use crate::{HerdrWindow, config::Config};
use chrono::{DateTime, Utc};
use gpui::*;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

impl HerdrWindow {
    /// Write one setting off the UI thread, then reload so the window shows
    /// what the file now says rather than what was clicked.
    pub(crate) fn save_preference(
        &mut self,
        save: impl FnOnce() -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.theme_save_in_flight() {
            return;
        }
        if self.config_load.is_some() {
            // A combined write and reload would be refused here, discarding
            // the click. The config lock serializes the write with other
            // saves, and the config watcher reloads once the busy load ends.
            self.write_preference(save, cx);
            return;
        }
        let light = crate::app::light_appearance(cx);
        let text_system = cx.text_system().clone();
        self.load_gui_config_with(
            move || {
                save()?;
                let mut config = Config::load()?;
                config.resolve_fonts(|| text_system.all_font_names());
                let theme = if config.theme == "Follow Herdr" {
                    Default::default()
                } else {
                    config.theme(light)?
                };
                Ok((config, theme))
            },
            cx,
        );
    }

    pub(crate) fn write_preference(
        &mut self,
        save: impl FnOnce() -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let saved = cx.background_executor().spawn(async move { save() });
        cx.spawn(async move |this, cx| {
            let result = saved.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => this.load_gui_config(cx),
                Err(error) => {
                    tracing::warn!(%error, "Could not save GUI preference");
                    this.local_error = Some(format!("Save GUI config: {error}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Asynchronous, endpoint-local preferences. Dropping lets the worker drain queued
/// saves without waiting; process exit may interrupt pending writes.
/// How the agents panel orders its rows, as in the terminal client: grouped
/// keeps the daemon's workspace order, priority floats the agents that want
/// attention. Herdr's `ui.agent_panel_sort` seeds it until the user toggles
/// it, which deserializes upstream's spellings ("spaces", alias "workspaces").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub enum AgentSort {
    #[default]
    #[serde(rename = "spaces", alias = "workspaces")]
    Grouped,
    #[serde(rename = "priority")]
    Priority,
}

impl std::fmt::Display for AgentSort {
    /// Also the stored spelling, which `AgentSort::parse` reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grouped => "grouped",
            Self::Priority => "priority",
        })
    }
}

impl AgentSort {
    pub fn toggled(self) -> Self {
        match self {
            Self::Grouped => Self::Priority,
            Self::Priority => Self::Grouped,
        }
    }

    /// An unreadable or unknown stored value is no override at all, so the
    /// daemon's setting applies rather than a guessed sort.
    fn parse(value: Option<&serde_json::Value>) -> Option<Self> {
        match value.and_then(serde_json::Value::as_str)? {
            "grouped" => Some(Self::Grouped),
            "priority" => Some(Self::Priority),
            _ => None,
        }
    }
}

/// The window chrome this endpoint remembers between runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {
    /// The activity bar's panel; `None` before one was picked.
    pub activity_panel: Option<crate::sidebar::ActivityPanel>,
    /// When each activity bar section was last open, so its count shows in
    /// the accent only for what changed since.
    pub activity_last_opened: HashMap<crate::sidebar::ActivityPanel, DateTime<Utc>>,
    pub sidebar_width: Option<f32>,
    pub sidebar_split: Option<f32>,
    /// The sort the user picked with the panel toggle, like upstream's
    /// `agent_panel_sort` preference. `None` follows the daemon's config.
    pub agent_sort: Option<AgentSort>,
    /// The notes panel's width, once dragged; `None` is its default.
    pub notes_width: Option<f32>,
    /// The review's file list's width, once dragged.
    pub review_files_width: Option<f32>,
    /// The device IDs in the order the user dragged them into.
    pub device_order: Vec<String>,
}

pub struct Preferences {
    saves: Option<Sender<Chrome>>,
    loaded: Option<Receiver<Chrome>>,
    worker: Option<JoinHandle<()>>,
}

impl Preferences {
    pub fn new(socket: &Path) -> Self {
        Self::start(
            state_dir()
                .map(|dir| endpoint_path(&dir, socket))
                .ok_or(crate::Error::MissingStateRoot),
        )
    }

    fn start(path: crate::Result<PathBuf>) -> Self {
        let (saves, requests) = mpsc::channel();
        let (loaded_tx, loaded) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("gpui-preferences".into())
            .spawn(move || {
                let path = match path {
                    Ok(path) => path,
                    Err(_) => {
                        tracing::warn!(
                            category = "preferences_location",
                            "Cannot locate GPUI preferences"
                        );
                        let _ = loaded_tx.send(Chrome::default());
                        return;
                    }
                };
                let chrome = match read_chrome(&path) {
                    Ok(chrome) => chrome,
                    Err(_) => {
                        tracing::warn!(
                            category = "preferences_read",
                            "Cannot read GPUI preferences"
                        );
                        Chrome::default()
                    }
                };
                let _ = loaded_tx.send(chrome);
                for chrome in requests {
                    if write_chrome(&path, chrome).is_err() {
                        tracing::warn!(
                            category = "preferences_write",
                            "Cannot save GPUI preferences"
                        );
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(category = "preferences_worker_start", error_kind = ?error.kind(), "Cannot start GPUI preferences worker");
                None
            }
        };
        Self {
            saves: Some(saves),
            loaded: Some(loaded),
            worker,
        }
    }

    /// Takes the initial result once; `None` means pending or already taken.
    /// Defaults stand in for a failed initial load.
    pub fn loaded(&mut self) -> Option<Chrome> {
        let result = match self.loaded.as_ref()?.try_recv() {
            Ok(chrome) => Some(chrome),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Some(Chrome::default()),
        };
        self.loaded = None;
        result
    }

    /// Queues the whole chrome, so saving one field never drops the others.
    pub fn save(&self, chrome: Chrome) {
        if let Some(saves) = &self.saves
            && saves.send(chrome).is_err()
        {
            tracing::warn!(
                category = "preferences_worker_disconnected",
                "Cannot queue GPUI preferences save"
            );
        }
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        // Disconnect and detach: the worker drains queued saves while the process
        // remains alive, without making the UI wait for disk I/O.
        self.saves.take();
        drop(self.worker.take());
    }
}

pub(crate) use herdr_client::local_path::state_dir;

fn endpoint_path(dir: &Path, socket: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in socket.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    dir.join(format!("local-{hash:016x}.json"))
}

fn read_chrome(path: &Path) -> crate::Result<Chrome> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Chrome::default()),
        Err(error) => return Err(error.into()),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let object = value
        .as_object()
        .ok_or(crate::config::Error::PreferencesNotObject)?;
    // Earlier builds always wrote `agent_sort`, so its default value cannot be
    // told from a choice; only "priority" proves the user toggled it.
    let agent_sort = AgentSort::parse(object.get("agent_sort_manual")).or_else(|| {
        AgentSort::parse(object.get("agent_sort")).filter(|sort| *sort == AgentSort::Priority)
    });
    let sidebar_width = match object.get("sidebar_width_px") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let width = value.as_f64().map(|width| width as f32);
            match width {
                Some(width) if width.is_finite() && width > 0.0 => Some(width),
                _ => return Err(crate::Error::InvalidStoredWidth),
            }
        }
    };
    let sidebar_split = object
        .get("sidebar_split")
        .and_then(serde_json::Value::as_f64)
        .map(|split| split as f32)
        .filter(|split| split.is_finite() && (0.1..=0.9).contains(split));
    // A damaged panel width is forgotten rather than failing the whole file.
    let panel_width = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .map(|width| width as f32)
            .filter(|width| width.is_finite() && *width > 0.0)
    };
    Ok(Chrome {
        activity_panel: object
            .get("activity_panel")
            .and_then(serde_json::Value::as_str)
            .and_then(crate::sidebar::ActivityPanel::parse),
        activity_last_opened: last_opened(object.get("activity_last_opened_at")),
        sidebar_width,
        sidebar_split,
        agent_sort,
        notes_width: panel_width("notes_width_px"),
        review_files_width: panel_width("review_files_width_px"),
        device_order: device_order(object.get("device_order")),
    })
}

/// When each section was last open. An unknown section or unreadable time is
/// forgotten rather than failing the whole file; there are few sections.
fn last_opened(
    value: Option<&serde_json::Value>,
) -> HashMap<crate::sidebar::ActivityPanel, DateTime<Utc>> {
    value
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(name, at)| {
            let panel = crate::sidebar::ActivityPanel::parse(name)?;
            let at = DateTime::parse_from_rfc3339(at.as_str()?).ok()?;
            Some((panel, at.with_timezone(&Utc)))
        })
        .collect()
}

/// The stored device order. Anything but a list of IDs is forgotten rather
/// than failing the whole file, and the list is bounded like the catalogs.
fn device_order(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_DEVICE_ID)
        .take(MAX_DEVICES)
        .map(str::to_owned)
        .collect()
}

/// Bounds on the stored device order: far more devices than anyone saves,
/// each ID far longer than a catalog key.
const MAX_DEVICES: usize = 1024;
const MAX_DEVICE_ID: usize = 1024;

fn write_chrome(path: &Path, chrome: Chrome) -> crate::Result<()> {
    let width = chrome.sidebar_width;
    if width.is_some_and(|width| !width.is_finite() || width <= 0.0) {
        return Err(crate::Error::InvalidSidebarWidth);
    }
    let parent = path.parent().ok_or(crate::Error::PreferencesPath)?;
    fs::create_dir_all(parent)?;
    let (temporary, mut file) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".preferences-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let result = (|| -> crate::Result<()> {
        let last_opened: serde_json::Map<String, serde_json::Value> = chrome
            .activity_last_opened
            .iter()
            .map(|(panel, at)| (panel.name().to_owned(), at.to_rfc3339().into()))
            .collect();
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({
                "activity_panel": chrome.activity_panel.map(crate::sidebar::ActivityPanel::name),
                "activity_last_opened_at": last_opened,
                "sidebar_width_px": width,
                "sidebar_split": chrome.sidebar_split.filter(|split| {
                    split.is_finite() && (0.1..=0.9).contains(split)
                }),
                "agent_sort_manual": chrome.agent_sort.map(|sort| sort.to_string()),
                "notes_width_px": chrome.notes_width.filter(|width| width.is_finite() && *width > 0.0),
                "review_files_width_px": chrome
                    .review_files_width
                    .filter(|width| width.is_finite() && *width > 0.0),
                "device_order": chrome.device_order,
            }),
        )?;
        file.write_all(b"\n")?;
        Ok(file.sync_all()?)
    })();
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path).map_err(crate::Error::from));
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
    {
        tracing::warn!(category = "preferences_cleanup", error_kind = ?error.kind(), "Cannot clean up GPUI preferences");
    }
    result
}

#[cfg(test)]
mod tests;

// A sibling of `tests`: its glob import shadows `#[test]` with GPUI's macro.
#[cfg(test)]
mod busy_load_tests {
    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn preference_clicked_during_a_config_load_is_still_written(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let written = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        view.update(cx, |view, cx| {
            // A load that never finishes, such as one the watcher started.
            view.config_load = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
            let written = written.clone();
            view.save_preference(
                move || {
                    written.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(written.load(std::sync::atomic::Ordering::SeqCst), 1);

        view.update(cx, |view, cx| {
            view.save_preference(
                || Err(crate::config::Error::InvalidTerminalPadding("padding_x").into()),
                cx,
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let error = view.local_error.as_deref().unwrap();
            assert!(error.starts_with("Save GUI config:"), "{error}");
            // The busy load is left alone; the watcher applies the write.
            assert!(view.config_load.is_some());
        });
    }
}

use crate::{
    HerdrWindow,
    config::{Config, Features},
    fonts::StyledFont,
};
use gpui::{prelude::*, *};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Debug selector, label, and state of each feature flag, in display order.
/// Flags are turned on in the config file, so Preferences only reports them.
pub(crate) fn feature_rows(features: &Features) -> [(&'static str, &'static str, bool); 1] {
    [(
        "preferences-feature-sidebar-hover-menu",
        "Sidebar hover menu",
        features.sidebar_hover_menu,
    )]
}

impl HerdrWindow {
    pub(super) fn render_preferences(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let accent = crate::menu::accent(theme);
        let section = |title: &'static str| {
            div()
                .pt(px(12.))
                .pb(px(6.))
                .text_size(px(font.size * 0.85))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(title)
        };
        let row = |id: &'static str, label: &'static str, value: String| {
            div()
                .debug_selector(move || id.into())
                .flex()
                .min_w_0()
                .gap(px(12.))
                .py(px(7.))
                .border_b_1()
                .border_color(rgb(theme.active))
                .child(
                    div()
                        .w(relative(0.3))
                        .flex_none()
                        .min_w_0()
                        .text_color(rgb(theme.muted))
                        .child(label),
                )
                .child(div().flex_1().min_w_0().text_right().child(value))
        };
        let note = |text: &'static str| {
            div()
                .min_w_0()
                .py(px(10.))
                .text_color(rgb(theme.muted))
                .child(text)
        };
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .min_w_0()
                .px(px(8.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.background))
                .text_color(accent)
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.active)))
                .child(label)
        };
        let mut body = div()
            .id("preferences-body")
            .debug_selector(|| "preferences-body".into())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.menu.preferences_scroll)
            .px(px(16.))
            .py(px(8.))
            .child(section("APPEARANCE"))
            .child(row(
                "preferences-show-agents",
                "Show agents",
                self.config.show_agents.to_string(),
            ))
            .child(row(
                "preferences-confirm-close-tab",
                "Confirm tab close",
                self.config.confirm_close_tab.to_string(),
            ))
            .child(row(
                "preferences-layout",
                "Layout",
                self.config.layout.mode.to_string(),
            ))
            .child(row(
                "preferences-sidebar-gap",
                "Sidebar gap",
                format!("{} px", self.config.layout.sidebar_gap),
            ))
            .child(row("preferences-theme", "Theme", self.config.theme.clone()))
            .child(div().py(px(10.)).child(
                button("preferences-choose-theme", "Choose theme").on_click(cx.listener(
                    |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_theme_picker(window, cx);
                    },
                )),
            ))
            .child(section("FONTS"));
        for (id, label, value) in [
            ("preferences-font-sidebar", "Sidebar", &self.config.sidebar),
            ("preferences-font-tabs", "Tabs", &self.config.tabs),
            (
                "preferences-font-terminal",
                "Terminal",
                &self.config.terminal,
            ),
            ("preferences-font-ui", "UI", &self.config.ui),
        ] {
            body = body.child(row(
                id,
                label,
                format!("{}, {} px", value.family, value.size),
            ));
        }
        body = body
            .child(section("NOTIFICATIONS"))
            .child(row("preferences-notifications-enabled", "In-app toasts", self.config.notifications.enabled.to_string()))
            .child(row("preferences-notifications-delay", "Delay (seconds)", self.config.notifications.delay_seconds.to_string()))
            .child(row("preferences-notifications-position", "Corner", format!("{:?}", self.config.notifications.position)))
            .child(note("Edit [notifications] in the local GUI config file; saved changes reload automatically. In-app notifications default off; QA previews always work. No sounds or OS notifications."))
            .child(note(
                "Font families and sizes are read-only here. Sizes are logical pixels, independent of display scaling.",
            ))
            .child(section("FEATURES"));
        for (id, label, enabled) in feature_rows(&self.config.features) {
            body = body.child(row(id, label, if enabled { "On" } else { "Off" }.into()));
        }
        body = body
            .child(note(
                "Optional behaviors, off by default. Turn one on in the [features] table of the local GUI config file; saved changes reload automatically.",
            ))
            .child(section("CONFIGURATION"))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .py(px(7.))
                    .child("GUI local overrides"),
            )
            .child(
                div()
                    .debug_selector(|| "preferences-config-path".into())
                    .w_full()
                    .min_w_0()
                    .p(px(10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.background))
                    .child(
                        Config::local_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|error| format!("Unavailable ({error})")),
                    ),
            )
            .child(note(
                "Edit this local file; saved changes reload automatically. Unset keys inherit config-gpui.toml, which is overwritten with current defaults on startup and reload. Invalid overrides leave the current appearance unchanged.",
            ))
            .child(
                button("preferences-reload-config", "Reload GUI config").on_click(cx.listener(
                    |this, _, window, cx| {
                        cx.stop_propagation();
                        this.reload_gui_config(window, cx);
                    },
                )),
            )
            .child(note(
                "Daemon configuration is separate. Reloading GUI config does not reload daemon settings.",
            ))
            .child(section("CONNECTION"))
            .child(row(
                "preferences-connection-status",
                "Status",
                self.live.status_text(self.local_error.as_deref()),
            ))
            .child(row(
                "preferences-connection-target",
                "Target",
                format!("{:?}", self.endpoints[self.selected_endpoint].connection.target),
            ));

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(font.line_height()))
            .text_color(rgb(theme.foreground))
            .child(
                div()
                    .debug_selector(|| "preferences-header".into())
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap(px(12.))
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex_none()
                            .w(px(3.))
                            .h(px(font.size * 2.5))
                            .rounded_full()
                            .bg(accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Preferences"),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme.muted))
                                    .child("Current GUI settings"),
                            ),
                    )
                    .child(
                        div()
                            .id("preferences-close")
                            .debug_selector(|| "preferences-close".into())
                            .flex_none()
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(|style| {
                                style
                                    .bg(rgb(theme.active))
                                    .text_color(rgb(theme.foreground))
                            })
                            .child("Close")
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    ),
            )
            .child(body)
            .child(
                div()
                    .debug_selector(|| "preferences-footer".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child("Esc to close  /  click outside to dismiss"),
            )
    }
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Asynchronous, endpoint-local preferences. Dropping lets the worker drain queued
/// saves without waiting; process exit may interrupt pending writes.
/// How the agents panel orders its rows, as in the terminal client: grouped
/// keeps the daemon's workspace order, priority floats the agents that want
/// attention. Client-local, like the sidebar width beside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentSort {
    #[default]
    Grouped,
    Priority,
}

impl std::fmt::Display for AgentSort {
    /// Also the stored spelling, which `From<&str>` reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grouped => "grouped",
            Self::Priority => "priority",
        })
    }
}

/// An unreadable or unknown preference falls back to the default rather than
/// failing the load, so this is infallible and not `FromStr`.
impl From<&str> for AgentSort {
    fn from(value: &str) -> Self {
        match value {
            "priority" => Self::Priority,
            _ => Self::Grouped,
        }
    }
}

impl AgentSort {
    pub fn toggled(self) -> Self {
        match self {
            Self::Grouped => Self::Priority,
            Self::Priority => Self::Grouped,
        }
    }

    fn parse(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(serde_json::Value::as_str)
            .map_or_else(Self::default, Self::from)
    }
}

/// Whether the sidebar lists rows at full width or collapses to a rail of
/// icons and status dots. Hiding it entirely is a separate, unsaved toggle, so
/// showing it again returns to whichever of these it was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarMode {
    #[default]
    Expanded,
    Collapsed,
}

impl std::fmt::Display for SidebarMode {
    /// Also the stored spelling, which `From<&str>` reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Expanded => "expanded",
            Self::Collapsed => "collapsed",
        })
    }
}

/// As with `AgentSort`, an unknown value falls back to the default.
impl From<&str> for SidebarMode {
    fn from(value: &str) -> Self {
        match value {
            "collapsed" => Self::Collapsed,
            _ => Self::Expanded,
        }
    }
}

impl SidebarMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Expanded => Self::Collapsed,
            Self::Collapsed => Self::Expanded,
        }
    }

    fn parse(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(serde_json::Value::as_str)
            .map_or_else(Self::default, Self::from)
    }
}

/// The window chrome this endpoint remembers between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    pub sidebar_width: Option<f32>,
    pub sidebar_split: Option<f32>,
    pub agent_sort: AgentSort,
    pub sidebar_mode: SidebarMode,
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

/// The GPUI client's own state directory, shared by preferences and logs.
pub(crate) fn state_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })
        .map(|root| root.join("herdr/gpui"))
}

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
        .ok_or(crate::Error::PreferencesNotObject)?;
    // A file written before the sort existed simply keeps the default.
    let agent_sort = AgentSort::parse(object.get("agent_sort"));
    let sidebar_mode = SidebarMode::parse(object.get("sidebar_mode"));
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
    Ok(Chrome {
        sidebar_width,
        sidebar_split,
        agent_sort,
        sidebar_mode,
    })
}

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
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({
                "sidebar_width_px": width,
                "sidebar_split": chrome.sidebar_split.filter(|split| {
                    split.is_finite() && (0.1..=0.9).contains(split)
                }),
                "agent_sort": chrome.agent_sort.to_string(),
                "sidebar_mode": chrome.sidebar_mode.to_string(),
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
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::time::{Duration, Instant};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            loop {
                let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir().join(format!(
                    "herdr-preferences-test-{}-{sequence}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create test directory: {error}"),
                }
            }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn await_loaded(preferences: &mut Preferences) -> Chrome {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(chrome) = preferences.loaded() {
                assert_eq!(preferences.loaded(), None);
                return chrome;
            }
            assert!(Instant::now() < deadline, "preferences load timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[core::prelude::v1::test]
    fn roundtrip_and_reset_drain_after_drop() {
        let directory = TestDirectory::new();
        let path = endpoint_path(&directory.0, Path::new("/tmp/test.sock"));
        let mut preferences = Preferences::start(Ok(path.clone()));
        assert_eq!(await_loaded(&mut preferences), Chrome::default());
        for width in 1..=100 {
            preferences.save(Chrome {
                sidebar_width: Some(width as f32),
                sidebar_split: Some(0.4),
                agent_sort: AgentSort::Priority,
                sidebar_mode: SidebarMode::Collapsed,
            });
        }
        let worker = preferences.worker.take().unwrap();
        drop(preferences);
        // Only the test waits for persistence before reading or removing files.
        worker.join().unwrap();
        let mut preferences = Preferences::start(Ok(path.clone()));
        assert_eq!(
            await_loaded(&mut preferences),
            Chrome {
                sidebar_width: Some(100.0),
                sidebar_split: Some(0.4),
                agent_sort: AgentSort::Priority,
                sidebar_mode: SidebarMode::Collapsed,
            }
        );
        preferences.save(Chrome::default());
        let worker = preferences.worker.take().unwrap();
        drop(preferences);
        worker.join().unwrap();
        assert_eq!(read_chrome(&path).unwrap(), Chrome::default());
        let mut preferences = Preferences::start(Ok(path));
        assert_eq!(await_loaded(&mut preferences), Chrome::default());
    }

    #[core::prelude::v1::test]
    fn stored_sorts_and_older_files_both_load() {
        let directory = TestDirectory::new();
        let path = endpoint_path(&directory.0, Path::new("/tmp/sort.sock"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A file from before the sort existed, then an unknown value.
        for (json, expected) in [
            (
                r#"{"sidebar_width_px": 240.0}"#,
                Chrome {
                    sidebar_width: Some(240.0),
                    sidebar_split: None,
                    agent_sort: AgentSort::Grouped,
                    sidebar_mode: SidebarMode::Expanded,
                },
            ),
            (
                r#"{"sidebar_width_px": null, "agent_sort": "sideways"}"#,
                Chrome::default(),
            ),
            (
                r#"{"sidebar_width_px": null, "sidebar_mode": "folded"}"#,
                Chrome::default(),
            ),
            (
                r#"{"sidebar_width_px": null, "sidebar_mode": 1}"#,
                Chrome::default(),
            ),
            (
                r#"{"sidebar_width_px": 200.0, "sidebar_mode": "collapsed"}"#,
                Chrome {
                    sidebar_width: Some(200.0),
                    sidebar_mode: SidebarMode::Collapsed,
                    ..Chrome::default()
                },
            ),
            (
                r#"{"sidebar_width_px": 200.0, "agent_sort": "priority"}"#,
                Chrome {
                    sidebar_width: Some(200.0),
                    sidebar_split: None,
                    agent_sort: AgentSort::Priority,
                    sidebar_mode: SidebarMode::Expanded,
                },
            ),
        ] {
            fs::write(&path, json).unwrap();
            assert_eq!(read_chrome(&path).unwrap(), expected, "{json}");
        }
        // Whatever was read survives a write and read of the same value.
        let chrome = Chrome {
            sidebar_width: Some(321.0),
            sidebar_split: None,
            agent_sort: AgentSort::Priority,
            sidebar_mode: SidebarMode::Collapsed,
        };
        write_chrome(&path, chrome).unwrap();
        assert_eq!(read_chrome(&path).unwrap(), chrome);
    }

    #[core::prelude::v1::test]
    fn drop_does_not_wait_for_blocked_worker_and_queued_saves_still_drain() {
        let (saves, requests) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let (drained_tx, drained) = mpsc::channel();
        let worker = thread::spawn(move || {
            ready_tx.send(()).unwrap();
            blocked.recv().unwrap();
            drained_tx
                .send(requests.into_iter().collect::<Vec<_>>())
                .unwrap();
        });
        let preferences = Preferences {
            saves: Some(saves),
            loaded: None,
            worker: Some(worker),
        };
        let queued = [
            Chrome {
                sidebar_width: Some(160.),
                sidebar_split: None,
                agent_sort: AgentSort::Grouped,
                sidebar_mode: SidebarMode::Expanded,
            },
            Chrome {
                sidebar_width: Some(400.),
                sidebar_split: Some(0.6),
                agent_sort: AgentSort::Priority,
                sidebar_mode: SidebarMode::Expanded,
            },
            Chrome::default(),
        ];
        for chrome in queued {
            preferences.save(chrome);
        }
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let (dropped_tx, dropped) = mpsc::channel();
        let dropper = thread::spawn(move || {
            drop(preferences);
            dropped_tx.send(()).unwrap();
        });
        let result = dropped.recv_timeout(Duration::from_secs(5));
        // Release even on failure so a regressed join does not strand the threads.
        release.send(()).unwrap();
        let saved = drained.recv_timeout(Duration::from_secs(5)).unwrap();
        dropper.join().unwrap();
        assert!(result.is_ok(), "drop waited for the blocked worker");
        assert_eq!(saved, queued);
    }

    #[core::prelude::v1::test]
    fn malformed_and_invalid_widths_fall_back_to_default() {
        let directory = TestDirectory::new();
        let path = directory.0.join("preferences.json");
        for contents in [
            "not json",
            "[]",
            "null",
            r#"{"sidebar_width_px":0}"#,
            r#"{"sidebar_width_px":-1}"#,
            r#"{"sidebar_width_px":"200"}"#,
            r#"{"sidebar_width_px":true}"#,
            r#"{"sidebar_width_px":1e100}"#,
            r#"{"sidebar_width_px":1e-100}"#,
            r#"{"sidebar_width_px":NaN}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert!(read_chrome(&path).is_err(), "accepted {contents}");
            let mut preferences = Preferences::start(Ok(path.clone()));
            assert_eq!(await_loaded(&mut preferences), Chrome::default());
        }
        for contents in ["{}", r#"{"sidebar_width_px":null}"#] {
            fs::write(&path, contents).unwrap();
            assert_eq!(read_chrome(&path).unwrap(), Chrome::default());
        }
        for width in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
            assert!(
                write_chrome(
                    &path,
                    Chrome {
                        sidebar_width: Some(width),
                        sidebar_split: None,
                        agent_sort: AgentSort::default(),
                        sidebar_mode: SidebarMode::Expanded,
                    }
                )
                .is_err()
            );
        }
        let chrome = Chrome {
            sidebar_width: Some(237.5),
            sidebar_split: None,
            agent_sort: AgentSort::default(),
            sidebar_mode: SidebarMode::Expanded,
        };
        write_chrome(&path, chrome).unwrap();
        assert_eq!(read_chrome(&path).unwrap(), chrome);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[core::prelude::v1::test]
    fn invalid_sidebar_splits_preserve_other_preferences() {
        let directory = TestDirectory::new();
        let path = directory.0.join("preferences.json");
        let expected = Chrome {
            sidebar_width: Some(240.0),
            sidebar_split: None,
            agent_sort: AgentSort::Priority,
            sidebar_mode: SidebarMode::Expanded,
        };
        for split in [
            "null", "0", "-1", "0.099", "0.901", "1e100", "1e-100", "\"0.5\"", "true", "[]", "{}",
        ] {
            fs::write(
                &path,
                format!(
                    r#"{{"sidebar_width_px":240,"agent_sort":"priority","sidebar_split":{split}}}"#
                ),
            )
            .unwrap();
            assert_eq!(read_chrome(&path).unwrap(), expected, "{split}");
        }
        for split in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            0.099,
            0.901,
        ] {
            write_chrome(
                &path,
                Chrome {
                    sidebar_split: Some(split),
                    ..expected
                },
            )
            .unwrap();
            assert_eq!(read_chrome(&path).unwrap(), expected, "{split}");
        }
    }

    #[core::prelude::v1::test]
    fn sidebar_split_roundtrips_including_boundaries_and_reset() {
        let directory = TestDirectory::new();
        let path = directory.0.join("preferences.json");
        for sidebar_split in [Some(0.1), Some(0.4), Some(0.9), None] {
            let chrome = Chrome {
                sidebar_width: Some(240.0),
                sidebar_split,
                agent_sort: AgentSort::Priority,
                sidebar_mode: SidebarMode::Expanded,
            };
            write_chrome(&path, chrome).unwrap();
            let stored: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(stored["sidebar_split"], serde_json::json!(sidebar_split));
            assert_eq!(read_chrome(&path).unwrap(), chrome);
        }
    }

    #[core::prelude::v1::test]
    fn endpoint_paths_use_stable_fnv1a() {
        let root = Path::new("/state/herdr/gpui");
        assert_eq!(
            endpoint_path(root, Path::new("hello")),
            Path::new("/state/herdr/gpui/local-a430d84680aabd0b.json")
        );
        assert_eq!(
            endpoint_path(root, Path::new("")),
            Path::new("/state/herdr/gpui/local-cbf29ce484222325.json")
        );
        assert_ne!(
            endpoint_path(root, Path::new("/a.sock")),
            endpoint_path(root, Path::new("/b.sock"))
        );
    }
}

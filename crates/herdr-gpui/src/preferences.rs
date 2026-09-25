use crate::{
    HerdrWindow,
    config::{Config, Features},
    menu::listener,
};
use gpui_kit::{
    component::{
        ActiveTheme as _, button::Button, description_list::DescriptionList, dialog::Dialog,
        group_box::GroupBox, h_flex, v_flex,
    },
    prelude::*,
    *,
};
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
    /// The current GUI settings, read-only: every value is edited in the
    /// config file, which reloads on save.
    pub(crate) fn preferences_dialog(
        &self,
        dialog: Dialog,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Dialog {
        let muted = cx.theme().muted_foreground;
        let note = |text: &'static str| div().text_sm().text_color(muted).child(text);
        let list = || DescriptionList::new().columns(1).label_width(px(160.));
        let appearance = list()
            .item("Show agents", self.config.show_agents.to_string(), 1)
            .item(
                "Confirm tab close",
                self.config.confirm_close_tab.to_string(),
                1,
            )
            .item("Layout", self.config.layout.mode.to_string(), 1)
            .item(
                "Sidebar gap",
                format!("{} px", self.config.layout.sidebar_gap),
                1,
            )
            .item("Theme", self.config.theme.clone(), 1);
        let fonts = [
            ("Sidebar", &self.config.sidebar),
            ("Tabs", &self.config.tabs),
            ("Terminal", &self.config.terminal),
            ("UI", &self.config.ui),
        ]
        .into_iter()
        .fold(list(), |fonts, (label, value)| {
            fonts.item(label, format!("{}, {} px", value.family, value.size), 1)
        });
        let notifications = list()
            .item(
                "In-app toasts",
                self.config.notifications.enabled.to_string(),
                1,
            )
            .item(
                "Delay (seconds)",
                self.config.notifications.delay_seconds.to_string(),
                1,
            )
            .item(
                "Corner",
                format!("{:?}", self.config.notifications.position),
                1,
            );
        let features = feature_rows(&self.config.features).into_iter().fold(
            list(),
            |features, (_, label, enabled)| {
                features.item(label, if enabled { "On" } else { "Off" }, 1)
            },
        );
        let connection = list()
            .item(
                "Status",
                self.live.status_text(self.local_error.as_deref()),
                1,
            )
            .item(
                "Target",
                format!(
                    "{:?}",
                    self.endpoints[self.selected_endpoint].connection.target
                ),
                1,
            );
        let path = Config::local_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| format!("Unavailable ({error})"));
        dialog
            .title("Preferences")
            .w(px(560.))
            .child(
                v_flex()
                    .debug_selector(|| "preferences-body".into())
                    .gap_4()
                    .child(
                        GroupBox::new()
                            .title("Appearance")
                            .child(appearance)
                            .child(
                                Button::new("preferences-choose-theme")
                                    .outline()
                                    .label("Choose theme")
                                    .on_click(listener(weak, |this, window, cx| {
                                        this.open_theme_picker(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        GroupBox::new()
                            .title("Fonts")
                            .child(fonts)
                            .child(note(
                                "Font families and sizes are read-only here. Sizes are logical pixels, independent of display scaling.",
                            )),
                    )
                    .child(
                        GroupBox::new()
                            .title("Notifications")
                            .child(notifications)
                            .child(note(
                                "Edit [notifications] in the local GUI config file; saved changes reload automatically. In-app notifications default off; QA previews always work. No sounds or OS notifications.",
                            )),
                    )
                    .child(
                        GroupBox::new()
                            .title("Features")
                            .child(features)
                            .child(note(
                                "Optional behaviors, off by default. Turn one on in the [features] table of the local GUI config file; saved changes reload automatically.",
                            )),
                    )
                    .child(
                        GroupBox::new()
                            .title("Configuration")
                            .child(list().item("GUI local overrides", path, 1))
                            .child(note(
                                "Edit this local file; saved changes reload automatically. Unset keys inherit config-gpui.toml, which is overwritten with current defaults on startup and reload. Invalid overrides leave the current appearance unchanged.",
                            ))
                            .child(
                                Button::new("preferences-reload-config")
                                    .outline()
                                    .label("Reload GUI config")
                                    .on_click(listener(weak, |this, window, cx| {
                                        this.reload_gui_config(window, cx)
                                    })),
                            )
                            .child(note(
                                "Daemon configuration is separate. Reloading GUI config does not reload daemon settings.",
                            )),
                    )
                    .child(GroupBox::new().title("Connection").child(connection)),
            )
            .footer(
                h_flex().w_full().justify_end().child(
                    Button::new("preferences-close")
                        .label("Close")
                        .on_click(listener(weak, |this, window, cx| {
                            this.dismiss_menu(window, cx)
                        })),
                ),
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

/// The window chrome this endpoint remembers between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    pub sidebar_width: Option<f32>,
    pub sidebar_split: Option<f32>,
    pub agent_sort: AgentSort,
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
                },
            ),
            (
                r#"{"sidebar_width_px": null, "agent_sort": "sideways"}"#,
                Chrome::default(),
            ),
            (
                r#"{"sidebar_width_px": 200.0, "agent_sort": "priority"}"#,
                Chrome {
                    sidebar_width: Some(200.0),
                    sidebar_split: None,
                    agent_sort: AgentSort::Priority,
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
            },
            Chrome {
                sidebar_width: Some(400.),
                sidebar_split: Some(0.6),
                agent_sort: AgentSort::Priority,
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
                    }
                )
                .is_err()
            );
        }
        let chrome = Chrome {
            sidebar_width: Some(237.5),
            sidebar_split: None,
            agent_sort: AgentSort::default(),
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

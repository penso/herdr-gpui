//! The daemon's own `config.toml`: where herdr keeps it, and the few keys the
//! app honors from it.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use super::{ClipboardToast, DaemonKeys, SidebarLayout};

/// The daemon's config is read for a handful of keys, so a file far larger
/// than any hand-written config is skipped rather than parsed on every load.
pub(super) const MAX_DAEMON_CONFIG_BYTES: u64 = 1 << 20;

/// The daemon's own config file, resolved exactly as herdr resolves it. Every
/// GUI reader of those settings shares this one answer.
pub(crate) fn daemon_config_path(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> PathBuf {
    if let Some(path) = get("HERDR_CONFIG_PATH") {
        return path.into();
    }
    let root = get("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(windows)]
            {
                if let Some(root) = get("APPDATA") {
                    return PathBuf::from(root);
                }
                get("HOME")
                    .or_else(|| get("USERPROFILE"))
                    .map(PathBuf::from)
                    // i18n: data
                    .map(|home| home.join("AppData/Roaming"))
                    .unwrap_or_else(env::temp_dir)
            }
            #[cfg(not(windows))]
            get("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
                .unwrap_or_else(env::temp_dir)
        });
    // Share production TUI settings even in a debug GUI build or SSH session.
    root.join("herdr/config.toml")
}

/// What the GUI honors from the daemon's own config.
#[derive(Clone, Debug, Default)]
pub(super) struct Daemon {
    pub(super) clipboard_toast: ClipboardToast,
    pub(super) keys: DaemonKeys,
    pub(super) sidebar_layout: SidebarLayout,
}

/// A config file the GUI does not own can hold anything, including settings
/// from a newer herdr, so only the keys read here matter and anything
/// unreadable, oversized, malformed, or unrecognized leaves the defaults alone.
pub(super) fn daemon_settings(path: &Path) -> Daemon {
    if fs::metadata(path).is_ok_and(|data| data.len() > MAX_DAEMON_CONFIG_BYTES) {
        return Daemon::default();
    }
    let Some(table) = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
    else {
        return Daemon::default();
    };
    Daemon {
        clipboard_toast: daemon_clipboard_toast(&table),
        keys: DaemonKeys::from_table(table.get("keys").and_then(toml::Value::as_table)),
        sidebar_layout: SidebarLayout::from_daemon_config(&table).unwrap_or_default(),
    }
}

fn daemon_clipboard_toast(table: &toml::Table) -> ClipboardToast {
    let mut resolved = ClipboardToast::default();
    let Some(clipboard) = table
        .get("ui")
        .and_then(|ui| ui.get("toast")?.get("clipboard")?.as_table())
    else {
        return resolved;
    };
    if let Some(enabled) = clipboard.get("enabled").and_then(toml::Value::as_bool) {
        resolved.enabled = enabled;
    }
    if let Some(position) = clipboard
        .get("position")
        .cloned()
        .and_then(|position| position.try_into().ok())
    {
        resolved.position = position;
    }
    resolved
}

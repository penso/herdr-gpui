//! The taskbar identity of Herdr's windows on Windows. Herdr starts through
//! `herdr-gpui-launcher.exe`, but its windows belong to the console-subsystem
//! `herdr-gpui.exe`, so a pinned window would reopen with a console. Each
//! top-level window instead carries an explicit AppUserModelID and relaunch
//! properties that point a pin at the launcher. Other platforms set nothing.
//! The button's badge and progress bar live in [`button`].
// i18n: migrated

use gpui::{App, Global, Window};

#[cfg(any(windows, test))]
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The windowed launcher, installed beside `herdr-gpui.exe`.
#[cfg(any(windows, test))]
const LAUNCHER_EXE: &str = "herdr-gpui-launcher.exe";

/// The name a pinned Herdr shows on the taskbar and in its Jump List.
#[cfg(any(windows, test))]
const DISPLAY_NAME: &str = "Herdr UI"; // i18n: data

/// What a window sets so that pinning it pins the launcher.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Relaunch {
    command: OsString,
    display_name: &'static str,
    icon: OsString,
    id: &'static str,
    launcher: PathBuf,
}

/// The relaunch properties for the `herdr-gpui.exe` at `gui`, or `None` when
/// it has no parent directory. Touches no filesystem.
#[cfg(any(windows, test))]
fn relaunch(gui: &Path) -> Option<Relaunch> {
    let launcher = gui.parent()?.join(LAUNCHER_EXE);
    let mut command = OsString::from("\"");
    command.push(&launcher);
    command.push("\"");
    // Resource ID 1 is the icon both executables embed.
    let mut icon = launcher.clone().into_os_string();
    icon.push(",-1");
    Some(Relaunch {
        command,
        display_name: DISPLAY_NAME,
        icon,
        id: crate::constants::APP_ID,
        launcher,
    })
}

/// Startup's verdict on whether windows get relaunch properties, and the
/// windows that carry them, so they are cleared before each one closes.
#[derive(Debug, Default)]
pub(crate) struct Taskbar {
    #[cfg(windows)]
    relaunch: Option<Relaunch>,
    #[cfg(windows)]
    windows: std::collections::HashMap<gpui::WindowId, native::Hwnd>,
}

impl Global for Taskbar {}

impl Taskbar {
    /// Decides, before GPUI starts, whether the launcher is installed beside
    /// this executable. Without it (a `cargo run` build, an old archive)
    /// windows get nothing, as before.
    #[cfg(windows)]
    pub(crate) fn load() -> Self {
        let gui = match std::env::current_exe() {
            Ok(gui) => gui,
            Err(error) => {
                tracing::warn!(%error, "Unable to locate herdr-gpui.exe; a pinned window will open it directly");
                return Self::default();
            }
        };
        let relaunch = relaunch(&gui).filter(|relaunch| relaunch.launcher.is_file());
        if relaunch.is_none() {
            tracing::info!(gui = %gui.display(), "No {LAUNCHER_EXE} beside the GUI; windows keep the default taskbar identity");
        }
        Self {
            relaunch,
            windows: Default::default(),
        }
    }

    /// The windowed launcher beside this executable, when it is installed.
    #[cfg(windows)]
    pub(crate) fn launcher(&self) -> Option<&Path> {
        self.relaunch
            .as_ref()
            .map(|relaunch| relaunch.launcher.as_path())
    }

    #[cfg(not(windows))]
    pub(crate) fn load() -> Self {
        Self::default()
    }

    /// Makes the verdict available to window sites and clears each marked
    /// window's properties when it closes.
    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        #[cfg(windows)]
        cx.on_window_closed(|cx, id| {
            let hwnd = cx
                .has_global::<Self>()
                .then(|| cx.global_mut::<Self>().windows.remove(&id))
                .flatten();
            if let Some(hwnd) = hwnd {
                native::clear(hwnd);
            }
        })
        .detach();
    }
}

/// Gives a newly built top-level `window` the relaunch properties, when the
/// launcher was found at startup.
#[cfg(windows)]
pub(crate) fn mark(window: &Window, cx: &mut App) {
    let Some(relaunch) = cx
        .try_global::<Taskbar>()
        .and_then(|taskbar| taskbar.relaunch.clone())
    else {
        return;
    };
    if let Some(hwnd) = native::apply(window, &relaunch) {
        cx.global_mut::<Taskbar>()
            .windows
            .insert(window.window_handle().window_id(), hwnd);
    }
}

#[cfg(not(windows))]
pub(crate) fn mark(_window: &Window, _cx: &mut App) {}

/// The button itself: the attention badge and progress.
#[cfg(windows)]
pub(crate) mod button;
#[cfg(windows)]
mod native;

#[cfg(test)]
mod tests;

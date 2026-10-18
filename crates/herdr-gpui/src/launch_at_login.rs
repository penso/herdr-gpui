//! **Launch at login** on Windows: a `Herdr UI` value under the current
//! user's `Run` key, naming `herdr-gpui-launcher.exe` so the app opens without
//! a console window. The registry is the setting: nothing is kept in the
//! config file, so a value removed by an uninstall, or by hand, reads as off.
//!
//! Task Manager's **Startup apps** and **Settings > Apps > Startup** turn an
//! entry off without deleting it, by marking it in `StartupApproved\Run`. The
//! app reads that mark and never clears it: switching the entry back on there
//! stays the user's choice. Turning the setting off removes both values.
//!
//! macOS and Linux have no such setting yet; this module is Windows-only.

use std::path::{Path, PathBuf};
use windows_registry::{CURRENT_USER, Key};

/// The value name, which Task Manager shows (with the launcher's icon).
const VALUE: &str = "Herdr UI";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const LAUNCHER: &str = "herdr-gpui-launcher.exe";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Launch at login needs {LAUNCHER} beside the app, at {}", path.display())]
    NoLauncher { path: PathBuf },
    #[error("Could not find the running executable")]
    CurrentExe(#[source] std::io::Error),
    #[error("Could not {operation} the startup entry")]
    Registry {
        operation: &'static str,
        #[source]
        source: windows::core::Error,
    },
}

/// What the startup entry looks like now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Off,
    On,
    /// Present, but turned off in Startup apps; Windows skips it at sign-in.
    DisabledByWindows,
}

impl State {
    /// `approved` is the entry's `StartupApproved\Run` data, if any. Windows
    /// writes 12 bytes: a flag word, then when it was turned off. An odd first
    /// byte (`03`, `07`) is off; `02`, `06`, and no data at all are on.
    fn of(run: bool, approved: Option<&[u8]>) -> Self {
        match (run, approved.and_then(<[u8]>::first)) {
            (false, _) => Self::Off,
            (true, Some(flags)) if flags & 1 == 1 => Self::DisabledByWindows,
            (true, _) => Self::On,
        }
    }
}

/// The `Run` value for a launcher: its path, quoted because it usually has a
/// space (`Programs\Herdr UI`).
fn command_line(launcher: &Path) -> String {
    format!("\"{}\"", launcher.display())
}

/// The launcher beside `exe`, the same one the Start menu shortcut opens.
fn launcher_beside(exe: &Path) -> Result<PathBuf, Error> {
    let path = exe.with_file_name(LAUNCHER);
    if path.is_file() {
        Ok(path)
    } else {
        Err(Error::NoLauncher { path })
    }
}

/// Registry access: call it off the UI thread.
pub(crate) fn state() -> Result<State, Error> {
    state_in(CURRENT_USER)
}

/// Writes or removes the entry for this executable's launcher. Registry
/// access: call it off the UI thread.
pub(crate) fn set(enabled: bool) -> Result<(), Error> {
    let launcher = if enabled {
        let exe = std::env::current_exe().map_err(Error::CurrentExe)?;
        Some(launcher_beside(&exe)?)
    } else {
        None
    };
    set_in(CURRENT_USER, launcher.as_deref())
}

/// `root` stands for `HKEY_CURRENT_USER`; tests pass a scratch key.
fn state_in(root: &Key) -> Result<State, Error> {
    let run = read(root, RUN, |key| key.get_value(VALUE).map(|_| ()))?.is_some();
    let approved = read(root, APPROVED, |key| {
        key.get_value(VALUE).map(|value| value.to_vec())
    })?;
    Ok(State::of(run, approved.as_deref()))
}

/// `Some` writes the entry for that launcher; `None` removes it, with any
/// Startup apps mark, so turning it on later starts clean.
fn set_in(root: &Key, launcher: Option<&Path>) -> Result<(), Error> {
    let failed = |operation| move |source| Error::Registry { operation, source };
    match launcher {
        Some(launcher) => root
            .create(RUN)
            .and_then(|key| key.set_string(VALUE, command_line(launcher)))
            .map_err(failed("write")),
        None => {
            remove(root, RUN)?;
            remove(root, APPROVED)
        }
    }
}

/// Runs `get` on `path`'s key; a missing key or value is `None`.
fn read<T>(
    root: &Key,
    path: &str,
    get: impl FnOnce(&Key) -> windows::core::Result<T>,
) -> Result<Option<T>, Error> {
    match root.open(path).and_then(|key| get(&key)) {
        Ok(value) => Ok(Some(value)),
        Err(error) if missing(&error) => Ok(None),
        Err(source) => Err(Error::Registry {
            operation: "read",
            source,
        }),
    }
}

fn remove(root: &Key, path: &str) -> Result<(), Error> {
    let key = match root.options().read().write().open(path) {
        Ok(key) => key,
        Err(error) if missing(&error) => return Ok(()),
        Err(source) => {
            return Err(Error::Registry {
                operation: "open",
                source,
            });
        }
    };
    match key.remove_value(VALUE) {
        Err(error) if !missing(&error) => Err(Error::Registry {
            operation: "remove",
            source: error,
        }),
        _ => Ok(()),
    }
}

fn missing(error: &windows::core::Error) -> bool {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
    [ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND]
        .into_iter()
        .any(|code| error.code() == windows::core::HRESULT::from_win32(code))
}

#[cfg(test)]
mod tests;

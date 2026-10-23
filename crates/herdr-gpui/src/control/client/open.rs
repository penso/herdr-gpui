//! `open DIR`: the folder, checked here, opened by the running app as its
//! Open Project does, starting the app first when none is running.
use super::{Context, Output};
use crate::control::{
    ErrorCode,
    protocol::{Request, app::AppOp},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long `open` waits, after starting the app, for its window to answer
/// and to reach the local Herdr. A cold start with a daemon to connect to
/// takes a few seconds; past this the app is reported as not answering.
pub(crate) const START_WAIT: Duration = Duration::from_secs(20);
/// The pause between attempts while waiting.
const RETRY: Duration = Duration::from_millis(250);

pub(super) fn open(path: &Path, context: &Context) -> crate::Result<Output> {
    let path = directory(path)?;
    let send = || context.act(Request::App, AppOp::Open { path: path.clone() });
    match send() {
        // A window ID names a window of a running app, so there is nothing
        // to start for one.
        Err(crate::Error::Control(crate::control::Error::Unavailable { .. }))
            if context.window.is_none() =>
        {
            start_app()?;
            retry(
                Instant::now() + START_WAIT,
                send,
                std::thread::sleep,
                Instant::now,
            )
        }
        result => result,
    }
}

/// Starts the app with no arguments, as a plain `herdr-gpui` would, and
/// returns once it has started. On Windows `new_command` gives it a console
/// with no window, so `open` run from the launcher, as Explorer's entry
/// does, flashes none, and closing the terminal `open` ran in leaves it.
fn start_app() -> std::io::Result<()> {
    let app = std::env::current_exe()?;
    #[cfg(windows)]
    keep_std_handles_from_children();
    herdr_client::new_command(&app)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

/// Stops this process's standard handles from passing to the app, which
/// outlives `open`: a pipe `open` writes to would otherwise stay open, and
/// whoever reads it waiting, until the app quits.
#[cfg(windows)]
fn keep_std_handles_from_children() {
    use windows_sys::Win32::{
        Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation},
        System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
    };
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle only reads this process's parameters. A null
        // or invalid result, or a handle that rejects the change, makes
        // SetHandleInformation fail without effect, which is ignored: the
        // handle then stays as it was. Clearing the flag changes only what
        // later children inherit, not this process's use of the handle.
        #[allow(unsafe_code)]
        unsafe {
            SetHandleInformation(GetStdHandle(which), HANDLE_FLAG_INHERIT, 0);
        }
    }
}

/// Sends until the app answers something other than "not there yet" or the
/// deadline passes, which reads as the app not answering in time.
fn retry(
    deadline: Instant,
    mut send: impl FnMut() -> crate::Result<Output>,
    mut sleep: impl FnMut(Duration),
    now: impl Fn() -> Instant,
) -> crate::Result<Output> {
    loop {
        sleep(RETRY);
        match send() {
            Err(error) if starting(&error) && now() < deadline => continue,
            Err(error) if starting(&error) => {
                return Err(crate::control::Error::Rejected {
                    code: ErrorCode::Timeout,
                    message: format!(
                        "Herdr UI did not open the folder within {} seconds of starting: {error}",
                        START_WAIT.as_secs()
                    ),
                }
                .into());
            }
            result => return result,
        }
    }
}

/// An app that is still starting: not listening yet, or its window not yet
/// connected to the local Herdr.
fn starting(error: &crate::Error) -> bool {
    matches!(
        error,
        crate::Error::Control(crate::control::Error::Unavailable { .. })
            | crate::Error::Control(crate::control::Error::Rejected {
                code: ErrorCode::Busy,
                ..
            })
    )
}

/// The folder as the app gets it: absolute, canonical, an existing
/// directory, and on a drive or share rather than a device. The path comes
/// from whoever runs the command, an Explorer verb included, so it is
/// checked here where the caller's directory is known and again in the app.
pub(crate) fn directory(path: &Path) -> crate::Result<String> {
    let refused = |code, message: String| {
        crate::Error::from(crate::control::Error::Rejected { code, message })
    };
    let path = &unquoted(path);
    let resolved: PathBuf = crate::palette::canonicalize(path).map_err(|error| {
        refused(
            ErrorCode::NotFound,
            format!("Cannot open {}: {error}", path.display()),
        )
    })?;
    if !resolved.is_dir() {
        return Err(refused(
            ErrorCode::InvalidRequest,
            format!("{} is not a folder", resolved.display()),
        ));
    }
    if device(&resolved) {
        return Err(refused(
            ErrorCode::InvalidRequest,
            format!("{} is a device, not a folder", resolved.display()),
        ));
    }
    resolved.into_os_string().into_string().map_err(|_| {
        refused(
            ErrorCode::InvalidRequest,
            "The folder's path must be UTF-8".into(),
        )
    })
}

/// The path with the backslash Windows' argument rules took back. Explorer
/// quotes the folder (`"%V"`), and for a drive root that is `"C:\"`, which
/// reaches the program as `C:"`: a backslash before a quote escapes it. No
/// Windows path can hold a quote, so a trailing one can only be that.
#[cfg(windows)]
fn unquoted(path: &Path) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_suffix('"')) {
        Some(text) => PathBuf::from(format!("{text}\\")),
        None => path.to_owned(),
    }
}

#[cfg(not(windows))]
fn unquoted(path: &Path) -> PathBuf {
    path.to_owned()
}

/// A path in a device namespace: `\\.\`, or `\\?\` other than a drive or
/// a share, such as `\\?\GLOBALROOT\Device\...`.
#[cfg(windows)]
fn device(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    !matches!(
        path.components().next(),
        Some(Component::Prefix(prefix)) if matches!(
            prefix.kind(),
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(..) | Prefix::VerbatimUNC(..)
        )
    )
}

/// The kernel's own trees, whose directories are not projects.
#[cfg(not(windows))]
fn device(path: &Path) -> bool {
    ["/dev", "/proc", "/sys"]
        .iter()
        .any(|root| path.starts_with(root))
}

#[cfg(test)]
mod tests;

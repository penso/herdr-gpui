//! Finding VS Code's command line, which runs `serve-web`. Blocking: it may
//! run the login shell for its `PATH`, so run it off the UI thread.
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// The command VS Code puts on `PATH`.
#[cfg(unix)]
const COMMAND: &str = "code";
#[cfg(windows)]
const COMMAND: &str = "code.cmd";

/// The program beside it that serves. On macOS and Windows `code` is a
/// launcher script that runs VS Code's Electron as Node, which runs this
/// program in turn; started directly, it is the app's own child, so stopping
/// that child stops the server, with no launcher left in between.
#[cfg(unix)]
const SERVER: &str = "code-tunnel";
#[cfg(windows)]
const SERVER: &str = "code-tunnel.exe";

/// Where VS Code keeps its command when it was never put on `PATH`.
#[cfg(target_os = "macos")]
const BUNDLED: &[&str] = &["/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"];
#[cfg(not(target_os = "macos"))]
const BUNDLED: &[&str] = &[];

/// The program that runs `serve-web`, when VS Code is installed.
pub(super) fn find() -> Option<PathBuf> {
    find_in(
        crate::login_env::path().as_deref(),
        BUNDLED.iter().map(Path::new),
    )
}

/// The first `code` on `path`, else the first of `bundled` there is, as the
/// program that serves.
fn find_in<'a>(
    path: Option<&OsStr>,
    bundled: impl IntoIterator<Item = &'a Path>,
) -> Option<PathBuf> {
    path.into_iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(COMMAND))
        .chain(bundled.into_iter().map(Path::to_path_buf))
        .find(|command| runnable(command))
        .map(|command| server(&command))
}

/// The program beside `command`, through any links to it, else `command`
/// itself: VS Code's standalone command line serves on its own.
fn server(command: &Path) -> PathBuf {
    let real = std::fs::canonicalize(command).unwrap_or_else(|_| command.to_owned());
    real.parent()
        .map(|dir| dir.join(SERVER))
        .filter(|server| runnable(server))
        .unwrap_or(real)
}

fn runnable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    metadata.is_file()
}

#[cfg(test)]
mod tests;

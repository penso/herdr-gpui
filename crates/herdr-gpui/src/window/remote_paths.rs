//! The path an SSH host reads for a file path one of its panes printed, and
//! the shell it reads it in. A Windows OpenSSH host prints drive and UNC
//! paths with either separator and reads them in PowerShell; any other host
//! reads POSIX paths in `/bin/sh`. Nothing here asks the host anything: `~`
//! and `..` are left for the host to resolve.

use super::file_links::FileLink;
use herdr_client::RemoteShell;

/// A file on an SSH host, as the host's fetch script names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemotePath {
    /// The `[user@]host` SSH target the pane's endpoint runs on.
    pub(crate) target: String,
    pub(crate) shell: RemoteShell,
    /// An absolute path in the shell's own form, or one under `~`.
    pub(crate) path: String,
    /// The line printed after the path.
    pub(crate) line: Option<u32>,
}

impl RemotePath {
    /// The host's file `link` names, when the pane runs on an SSH host and
    /// the path is absolute, under `~`, or relative to an absolute working
    /// directory.
    pub(crate) fn from_link(link: &FileLink) -> Option<Self> {
        let target = link.remote.clone()?;
        let (shell, path) = resolve(&link.path, link.cwd.as_deref())?;
        Some(Self {
            target,
            shell,
            path,
            line: link.line,
        })
    }

    /// The host's file, as the transfer functions name it.
    pub(crate) fn remote_file(&self) -> herdr_client::RemoteFile<'_> {
        herdr_client::RemoteFile {
            target: &self.target,
            shell: self.shell,
            path: &self.path,
        }
    }
}

/// The shell and the absolute or `~` path for `path` as printed under `cwd`.
fn resolve(path: &str, cwd: Option<&str>) -> Option<(RemoteShell, String)> {
    let path = file_url_drive(path);
    if windows(path) {
        return Some((RemoteShell::PowerShell, backslashes(path)));
    }
    if path.starts_with('/') {
        return Some((RemoteShell::Posix, path.to_owned()));
    }
    let cwd = cwd.map(file_url_drive);
    if let Some(rest) = path.strip_prefix('~') {
        // `~user` names another account's home, which is not resolved.
        if !(rest.is_empty() || rest.starts_with(['/', '\\'])) {
            return None;
        }
        let powershell = cwd.map_or(rest.starts_with('\\'), windows);
        return Some(if powershell {
            (RemoteShell::PowerShell, backslashes(path))
        } else {
            (RemoteShell::Posix, path.to_owned())
        });
    }
    // Empty, or relative to the root of a drive no cwd names.
    if path.is_empty() || path.starts_with('\\') {
        return None;
    }
    let relative = without_current(path);
    let cwd = cwd?;
    if windows(cwd) {
        let cwd = backslashes(cwd);
        let folder = cwd.trim_end_matches('\\');
        Some((
            RemoteShell::PowerShell,
            format!("{folder}\\{}", backslashes(relative)),
        ))
    } else if cwd.starts_with('/') {
        let folder = cwd.trim_end_matches('/');
        Some((RemoteShell::Posix, format!("{folder}/{relative}")))
    } else {
        None
    }
}

/// Whether `path` starts with a drive and a separator, as in `C:\` or `c:/`,
/// or is a UNC path such as `\\server\share`.
fn windows(path: &str) -> bool {
    path.starts_with(r"\\")
        || matches!(path.as_bytes(), [letter, b':', b'\\' | b'/', ..] if letter.is_ascii_alphabetic())
}

/// A `file://` address's drive path, which reads `/C:/…` on a Unix client,
/// as the drive path `C:/…`; any other path unchanged.
fn file_url_drive(path: &str) -> &str {
    match path.strip_prefix('/') {
        Some(rest) if windows(rest) && !rest.starts_with('\\') => rest,
        _ => path,
    }
}

/// `path` without the `./` or `.\` that only says it is relative.
fn without_current(mut path: &str) -> &str {
    while let Some(rest) = path.strip_prefix("./").or_else(|| path.strip_prefix(r".\")) {
        path = rest;
    }
    path
}

fn backslashes(path: &str) -> String {
    path.replace('/', "\\")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

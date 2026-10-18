//! Canonical project paths in the form a shell can start in.
//!
//! On Windows `fs::canonicalize` answers with a verbatim path
//! (`\\?\C:\work`). Handed to Herdr as a workspace's directory, that prefix
//! reaches the shell: PowerShell starts in
//! `Microsoft.PowerShell.Core\FileSystem::\\?\C:\work`, and cmd.exe cannot
//! start in it at all. Where the plain form names the same folder it is used
//! instead; a path too long for it keeps the prefix, which is the only way
//! to reach it.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// `fs::canonicalize`, without a verbatim prefix the path does not need.
pub(crate) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path).map(plain)
}

/// The form [`canonicalize`] gives `path`, which must already be canonical
/// in `fs::canonicalize`'s own form, verbatim prefix included. Discovery
/// lists a root's children under the root's verbatim path and passes each
/// through this, so a child too long for the plain form, or whose name ends
/// in a dot or space, keeps the prefix just as validating it later does.
pub(crate) fn plain_form(path: PathBuf) -> PathBuf {
    plain(path)
}

/// The longest path Win32 accepts without the verbatim prefix.
#[cfg(windows)]
const MAX_PATH: usize = 260;

/// `path` without its verbatim prefix, when the plain form means the same
/// thing: a drive or share path short enough for Win32, whose components
/// have no trailing dot or space, which plain paths silently drop.
#[cfg(windows)]
fn plain(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};
    let Some(text) = path.to_str() else {
        return path;
    };
    let rest = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(_) => text.strip_prefix(r"\\?\").map(str::to_owned),
            Prefix::VerbatimUNC(..) => text
                .strip_prefix(r"\\?\UNC\")
                .map(|rest| format!(r"\\{rest}")),
            _ => None,
        },
        _ => None,
    };
    let Some(rest) = rest else {
        return path;
    };
    let safe = rest.len() < MAX_PATH
        && Path::new(&rest)
            .components()
            .all(|component| match component {
                Component::Normal(name) => name
                    .to_str()
                    .is_some_and(|name| !name.ends_with(['.', ' '])),
                _ => true,
            });
    if safe { PathBuf::from(rest) } else { path }
}

#[cfg(not(windows))]
fn plain(path: PathBuf) -> PathBuf {
    path
}

#[cfg(test)]
mod tests;

//! Finding the programs that ship with this app, such as `herdr-acp` and
//! `herdr-remote-control`: beside its executable, as the bundle and `cargo build`
//! lay them out, else on the login shell's `PATH`.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// `program`, a file name with the platform's extension, beside this app's
/// executable or on `PATH`. Blocking: run it off the UI thread.
pub(crate) fn locate(program: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    locate_in(program, exe.as_deref(), crate::login_env::path().as_deref())
}

pub(crate) fn locate_in(
    program: &str,
    exe: Option<&Path>,
    path: Option<&OsStr>,
) -> Option<PathBuf> {
    // Keep both generations of bundled helpers ahead of unrelated PATH copies.
    let renamed = program.strip_prefix("herdr-").and_then(|name| {
        matches!(
            name.strip_suffix(".exe").unwrap_or(name),
            "acp" | "automate" | "nearby" | "remote-control"
        )
        .then(|| format!("sheprd-{name}"))
    });
    for (exe, path) in [(exe, None), (None, path)] {
        for name in renamed
            .as_deref()
            .into_iter()
            .chain(std::iter::once(program))
        {
            if let Some(found) = locate_exact(name, exe, path) {
                return Some(found);
            }
        }
    }
    None
}

fn locate_exact(program: &str, exe: Option<&Path>, path: Option<&OsStr>) -> Option<PathBuf> {
    let beside = exe.into_iter().flat_map(|exe| {
        // `cargo install` and package managers link the app into a bin
        // directory; its sibling is beside the real file.
        let real = std::fs::canonicalize(exe).ok();
        [
            exe.parent().map(Path::to_owned),
            real.and_then(|real| real.parent().map(Path::to_owned)),
        ]
    });
    beside
        .flatten()
        .chain(
            path.into_iter()
                .flat_map(std::env::split_paths)
                .filter(|dir| dir.is_absolute()),
        )
        .map(|dir| dir.join(program))
        .find(|candidate| runnable(candidate))
}

#[cfg(test)]
mod tests;

fn runnable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    metadata.is_file()
}

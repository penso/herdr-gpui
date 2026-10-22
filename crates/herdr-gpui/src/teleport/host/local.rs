//! Programs on this machine, found the way a host script's prelude finds
//! them, so a local host needs no shell and runs on every client platform.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// The user's tool directories under `home`, then `path`, in the order the
/// script prelude puts them on `PATH`. A program started directly gets them
/// as its `PATH` too, so a wrapper finds its interpreter where the program was.
pub(super) fn search_path(home: Option<&Path>, path: Option<&OsStr>) -> Vec<PathBuf> {
    let tools = home.into_iter().flat_map(|home| {
        [
            ".local/bin",
            ".local/share/mise/shims",
            ".bun/bin",
            ".cargo/bin",
        ]
        .map(|dir| home.join(dir))
    });
    let system = crate::local_path::HOMEBREW_DIRS
        .iter()
        .copied()
        .map(PathBuf::from);
    let nix = home.map(|home| home.join(".nix-profile/bin"));
    tools
        .chain(system)
        .chain(nix)
        .chain(path.into_iter().flat_map(std::env::split_paths))
        .filter(|dir| dir.is_absolute())
        .collect()
}

/// `program` in the first of `dirs` that has it. On Windows a bare name
/// also matches its `.exe`, `.cmd`, or `.bat`.
pub(super) fn find(program: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) && Path::new(program).extension().is_none() {
        [".exe", ".cmd", ".bat"]
            .map(|extension| format!("{program}{extension}"))
            .into()
    } else {
        vec![program.to_owned()]
    };
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| runnable(candidate))
}

#[cfg(unix)]
fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// No real machine has these, so the fixed system directories searched
    /// ahead of `PATH` cannot shadow the test's own fixtures.
    const TOOL: &str = "herdr-gpui-test-tool";
    const ABSENT: &str = "herdr-gpui-test-absent";

    fn lookup(program: &str, home: Option<&Path>, path: &OsStr) -> Option<PathBuf> {
        find(program, &search_path(home, Some(path)))
    }

    fn tool(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let file = dir.join(name);
        std::fs::write(&file, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn user_tools_come_before_path() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let bin = dir.path().join("bin");
        let name = if cfg!(windows) {
            format!("{TOOL}.exe")
        } else {
            TOOL.to_owned()
        };
        tool(&home.join(".local/bin"), &name);
        tool(&bin, &name);
        let path = std::env::join_paths([&bin]).unwrap();
        let found = lookup(TOOL, Some(&home), &path).unwrap();
        assert!(found.starts_with(&home), "{found:?}");
        assert_eq!(lookup(TOOL, None, &path).unwrap(), bin.join(name));
        assert_eq!(lookup(ABSENT, Some(&home), &path), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_is_not_executable_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(TOOL), "").unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(lookup(TOOL, None, &path), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_command_script_is_found_by_its_bare_name() {
        let dir = tempfile::tempdir().unwrap();
        let script = format!("{TOOL}.cmd");
        tool(dir.path(), &script);
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(lookup(TOOL, None, &path).unwrap(), dir.path().join(script));
    }
}

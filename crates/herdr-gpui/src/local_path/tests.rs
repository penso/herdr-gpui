use std::path::{Path, PathBuf};

use super::*;

#[cfg(target_os = "macos")]
mod macos;

fn fallback() -> Vec<PathBuf> {
    std::env::split_paths(&local_path()).collect()
}

#[test]
fn fallback_includes_homebrew() {
    let paths = fallback();
    assert!(
        paths
            .iter()
            .any(|path| path == Path::new("/opt/homebrew/bin"))
    );
    #[cfg(target_os = "linux")]
    assert!(
        paths
            .iter()
            .any(|path| path == Path::new("/home/linuxbrew/.linuxbrew/bin"))
    );
}

/// Linuxbrew is appended so an existing `/usr/local` install keeps precedence.
#[cfg(target_os = "linux")]
#[test]
fn linuxbrew_follows_usr_local() {
    let paths = fallback();
    let position = |dir: &str| paths.iter().position(|path| path == Path::new(dir));
    assert!(position("/usr/local/bin") < position("/home/linuxbrew/.linuxbrew/bin"));
}

#[test]
fn fallback_includes_system_tools_once() {
    let paths = fallback();
    for system in ["/usr/sbin", "/sbin"] {
        let system = Path::new(system);
        assert_eq!(paths.iter().filter(|dir| *dir == system).count(), 1);
    }
}

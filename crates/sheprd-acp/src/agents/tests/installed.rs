use super::*;

#[cfg(unix)]
#[test]
fn only_agents_with_their_own_command_are_installed() {
    let dir = tempfile::tempdir().unwrap();
    // npx alone would only download Claude's and Codex's adapters.
    install(dir.path(), "npx");
    install(dir.path(), "opencode");
    install(dir.path(), "codex-acp");
    let path = dir.path().as_os_str();
    assert_eq!(installed(Some(path)), [Agent::Codex, Agent::Opencode]);
    assert!(installed(None).is_empty());
}

#[test]
fn the_search_path_adds_user_and_system_folders_after_path() {
    let home = Path::new("/home/me");
    let path = search_path(Some(OsStr::new("/usr/bin")), Some(home));
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert_eq!(dirs[0], PathBuf::from("/usr/bin"));
    assert!(dirs.contains(&home.join(".local/bin")));
    assert!(dirs.contains(&home.join(".opencode/bin")));
    let without_home = search_path(None, None);
    assert!(std::env::split_paths(&without_home).all(|dir| !dir.starts_with("/home/me")));
}

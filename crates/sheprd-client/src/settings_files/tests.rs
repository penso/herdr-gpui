use super::*;

#[test]
fn state_follows_which_files_exist_and_whether_they_match() {
    assert_eq!(State::of(None, None), State::Fresh);
    assert_eq!(State::of(Some("a"), None), State::Personal);
    assert_eq!(State::of(None, Some("a")), State::Overrides);
    assert_eq!(State::of(Some("a"), Some("a")), State::Moving);
    assert_eq!(State::of(Some("a"), Some("b")), State::Conflict);
}

#[test]
fn only_a_file_without_the_header_is_personal() {
    assert_eq!(personal("theme = \"dark\"\n"), Some("theme = \"dark\"\n"));
    assert_eq!(personal(&format!("{MANAGED_HEADER}\n# x\n")), None);
    assert_eq!(personal(&format!("{MANAGED_HEADER}\r\n# x\r\n")), None);
}

#[test]
fn a_missing_file_reads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read(&dir.path().join("none.toml")).unwrap(), None);
}

#[test]
fn create_refuses_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    create(&path, "first").unwrap();
    assert_eq!(
        create(&path, "second").unwrap_err().kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "first");
}

#[cfg(unix)]
#[test]
fn create_keeps_a_link_to_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let target = dir.path().join("dotfiles/config.toml");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert_eq!(read(&path).unwrap(), None);
    assert!(create(&path, "seed").is_err());
    assert_eq!(fs::read_link(&path).unwrap(), target);
}

#[cfg(unix)]
#[test]
fn replace_writes_through_a_link_and_keeps_the_file_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.toml");
    fs::write(&target, "old").unwrap();
    let path = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    replace(&path, "new").unwrap();
    assert_eq!(fs::read_link(&path).unwrap(), target);
    assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

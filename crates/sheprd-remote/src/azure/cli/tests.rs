#![allow(clippy::expect_used)]
use super::*;

fn make_executable(path: &Path) {
    std::fs::write(path, "").expect("write CLI stand-in");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("mark CLI stand-in executable");
    }
}

#[test]
fn finds_the_cli_in_the_first_directory_that_has_it() {
    let empty = tempfile::tempdir().expect("tempdir");
    let with_cli = tempfile::tempdir().expect("tempdir");
    let name = NAMES[0];
    make_executable(&with_cli.path().join(name));

    let found = find_in([empty.path().to_owned(), with_cli.path().to_owned()]);

    assert_eq!(found, Some(with_cli.path().join(name)));
}

#[test]
fn skips_directories_without_the_cli() {
    let empty = tempfile::tempdir().expect("tempdir");
    assert_eq!(find_in([empty.path().to_owned()]), None);
}

#[cfg(windows)]
#[test]
fn finds_the_windows_batch_script() {
    let dir = tempfile::tempdir().expect("tempdir");
    make_executable(&dir.path().join("az.cmd"));

    assert_eq!(
        find_in([dir.path().to_owned()]),
        Some(dir.path().join("az.cmd"))
    );
}

#[cfg(unix)]
#[test]
fn skips_a_cli_that_is_not_executable() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("az"), "").expect("write");
    assert_eq!(find_in([dir.path().to_owned()]), None);
}

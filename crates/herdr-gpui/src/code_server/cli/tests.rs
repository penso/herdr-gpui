#![allow(clippy::unwrap_used)]
use super::*;

/// Writes a program the search accepts at `path`.
fn program(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    #[cfg(unix)]
    crate::test_executable::write(path, "#!/bin/sh\n", 0o755).unwrap();
    #[cfg(windows)]
    std::fs::write(path, "@echo off\r\n").unwrap();
}

fn joined(dirs: &[&Path]) -> std::ffi::OsString {
    std::env::join_paths(dirs).unwrap()
}

#[test]
fn the_login_path_comes_before_the_app_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let (empty, first, second) = (
        temp.path().join("empty"),
        temp.path().join("first"),
        temp.path().join("second"),
    );
    std::fs::create_dir_all(&empty).unwrap();
    program(&first.join(COMMAND));
    program(&second.join(COMMAND));
    let bundled = temp.path().join("bundle").join(COMMAND);
    program(&bundled);
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap();

    // The first directory on PATH that has it wins over later ones.
    let path = joined(&[&empty, &first, &second]);
    assert_eq!(
        find_in(Some(&path), [bundled.as_path()]),
        Some(canonical(&first.join(COMMAND)))
    );
    // The bundle is the fallback when PATH has none.
    let path = joined(&[&empty]);
    assert_eq!(
        find_in(Some(&path), [bundled.as_path()]),
        Some(canonical(&bundled))
    );
    assert_eq!(
        find_in(None, [bundled.as_path()]),
        Some(canonical(&bundled))
    );
    // Nothing installed finds nothing.
    let missing = temp.path().join("missing").join(COMMAND);
    assert_eq!(find_in(Some(&path), [missing.as_path()]), None);
}

#[test]
fn the_serving_program_beside_the_command_is_preferred() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("app").join("bin");
    program(&bin.join(COMMAND));
    let path = joined(&[&bin]);
    let canonical = std::fs::canonicalize(&bin).unwrap();
    assert_eq!(find_in(Some(&path), []), Some(canonical.join(COMMAND)));
    program(&bin.join(SERVER));
    assert_eq!(find_in(Some(&path), []), Some(canonical.join(SERVER)));
}

/// Homebrew and VS Code's own "Install 'code' command in PATH" link the
/// command into a PATH directory; the server is beside the link's target.
#[cfg(unix)]
#[test]
fn a_linked_command_finds_the_server_beside_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("app").join("bin");
    program(&bin.join(COMMAND));
    program(&bin.join(SERVER));
    let links = temp.path().join("links");
    std::fs::create_dir_all(&links).unwrap();
    std::os::unix::fs::symlink(bin.join(COMMAND), links.join(COMMAND)).unwrap();
    let path = joined(&[&links]);
    assert_eq!(
        find_in(Some(&path), []),
        Some(std::fs::canonicalize(&bin).unwrap().join(SERVER))
    );
}

#[cfg(unix)]
#[test]
fn what_cannot_run_is_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain");
    std::fs::create_dir_all(plain.join(COMMAND)).unwrap();
    let unexecutable = temp.path().join("unexecutable");
    std::fs::create_dir_all(&unexecutable).unwrap();
    crate::test_executable::write(unexecutable.join(COMMAND), "#!/bin/sh\n", 0o644).unwrap();
    let real = temp.path().join("real");
    program(&real.join(COMMAND));
    // A relative directory would depend on where the app was started.
    let path = joined(&[&plain, &unexecutable, Path::new("relative"), &real]);
    assert_eq!(
        find_in(Some(&path), []),
        Some(std::fs::canonicalize(&real).unwrap().join(COMMAND))
    );
}

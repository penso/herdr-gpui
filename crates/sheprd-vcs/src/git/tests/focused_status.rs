use super::*;

#[test]
fn a_tracked_binary_edit_is_dirty_with_no_lines_and_commits() {
    let (directory, _hooks, checkout, input) = repository();
    let logo = directory.path().join("logo.bin");
    std::fs::write(&logo, b"\0\x01\x02binary").unwrap();
    run(&checkout, &["add", "logo.bin"]);
    run(&checkout, &["commit", "-m", "add logo"]);
    std::fs::write(&logo, b"\0\x03\x04changed").unwrap();
    let focused = refresh(&input);
    assert_eq!(focused.lines, status(0, 0, 0));
    assert!(focused.dirty);
    assert_head_moves(&checkout, "binary");
}

#[test]
fn an_executable_bit_change_is_dirty_with_no_lines_and_commits() {
    let (directory, _hooks, checkout, input) = repository();
    mark_executable(&directory.path().join("tracked"));
    run(&checkout, &["update-index", "--chmod=+x", "tracked"]);
    let focused = refresh(&input);
    assert_eq!(focused.lines, status(0, 0, 0));
    assert!(focused.dirty);
    assert_head_moves(&checkout, "mode");
}

#[test]
fn a_staged_empty_file_is_dirty_with_no_lines_and_commits() {
    let (directory, _hooks, checkout, input) = repository();
    std::fs::write(directory.path().join("empty"), b"").unwrap();
    run(&checkout, &["add", "empty"]);
    let focused = refresh(&input);
    assert_eq!(focused.lines, status(0, 0, 0));
    assert!(focused.dirty);
    assert_head_moves(&checkout, "empty");
}

#[test]
fn a_pure_rename_is_dirty_with_no_lines_and_commits() {
    let (_directory, _hooks, checkout, input) = repository();
    run(&checkout, &["mv", "tracked", "renamed"]);
    let focused = refresh(&input);
    assert_eq!(focused.lines, status(0, 0, 0));
    assert!(focused.dirty);
    assert_head_moves(&checkout, "rename");
}

/// Linux can store a path that is not UTF-8. `git status -z` returns those
/// bytes, and the dirty check must not refuse the commit because of them.
#[cfg(target_os = "linux")]
#[test]
fn a_non_utf8_filename_stays_dirty_and_commits() {
    use std::os::unix::ffi::OsStringExt;
    let (directory, _hooks, checkout, input) = repository();
    let name = std::ffi::OsString::from_vec(b"caf\xff".to_vec());
    std::fs::write(directory.path().join(&name), b"one\n").unwrap();
    let added = Command::new("git")
        .args(["-C", &checkout, "add", "--"])
        .arg(&name)
        .status()
        .unwrap();
    assert!(added.success());
    run(&checkout, &["commit", "-m", "add bytes"]);
    std::fs::write(directory.path().join(&name), b"two\n").unwrap();
    let focused = refresh(&input);
    assert!(focused.dirty);
    assert_head_moves(&checkout, "bytes");
}

#[test]
fn a_clean_tree_has_nothing_to_commit() {
    let (_directory, _hooks, checkout, input) = repository();
    let focused = refresh(&input);
    assert_eq!(focused.lines, status(0, 0, 0));
    assert!(!focused.dirty);
    let before = head(&checkout);
    let error = commit(
        &checkout,
        "nothing",
        Instant::now() + Duration::from_secs(30),
        &|| false,
    )
    .unwrap_err();
    assert!(matches!(error, crate::Error::Git(Error::NothingToCommit)));
    assert_eq!(head(&checkout), before);
}

#[test]
fn a_text_edit_keeps_its_line_counts_and_commits() {
    let (directory, _hooks, checkout, input) = repository();
    std::fs::write(directory.path().join("tracked"), "changed\n").unwrap();
    let focused = refresh(&input);
    assert_eq!(focused.lines, status(1, 1, 0));
    assert!(focused.dirty);
    assert_head_moves(&checkout, "text");
}

#[test]
fn a_focused_zero_line_status_keeps_the_dirty_bit_in_the_cache() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.git.track(Some(input("feature")), true, now);
    peer.git.poll(now);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Status));
    peer.complete(
        requested,
        Completion::Status(Ok(Focused {
            lines: status(0, 0, 0),
            dirty: true,
        })),
        now,
    );
    assert_eq!(peer.git.dirty("/repo/.git", "feature"), Some(true));
    assert_eq!(
        peer.git.counts("/repo/.git", "feature"),
        Some(status(0, 0, 0))
    );
    assert_eq!(peer.git.uncommitted(), Some(true));
    assert_eq!(peer.git.status(), Some(status(0, 0, 0)));

    let due = now + REFRESH;
    peer.git.track(Some(input("feature")), true, due);
    peer.git.poll(due);
    let (requested, _) = peer.request();
    peer.complete(
        requested,
        Completion::Status(Ok(Focused {
            lines: status(0, 0, 0),
            dirty: false,
        })),
        due,
    );
    assert_eq!(peer.git.dirty("/repo/.git", "feature"), Some(false));
    assert_eq!(peer.git.uncommitted(), Some(false));
    assert_eq!(
        peer.git.counts("/repo/.git", "feature"),
        Some(status(0, 0, 0))
    );

    let failed = due + REFRESH;
    peer.git.track(Some(input("feature")), true, failed);
    peer.git.poll(failed);
    let (requested, _) = peer.request();
    peer.complete(
        requested,
        Completion::Status(Err(Error::NoCheckout.into())),
        failed,
    );
    assert_eq!(peer.git.status(), None);
    assert_eq!(peer.git.uncommitted(), None);
    assert_eq!(peer.git.dirty("/repo/.git", "feature"), None);
}

fn mark_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        std::fs::set_permissions(path, permissions).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn repository() -> (tempfile::TempDir, tempfile::TempDir, String, Input) {
    let directory = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    let checkout = directory.path().to_str().unwrap().to_owned();
    run(&checkout, &["init", "-b", "test"]);
    run(&checkout, &["config", "user.name", "Test"]);
    run(&checkout, &["config", "user.email", "test@example.invalid"]);
    run(&checkout, &["config", "commit.gpgsign", "false"]);
    run(
        &checkout,
        &["config", "core.hooksPath", hooks.path().to_str().unwrap()],
    );
    std::fs::write(directory.path().join("tracked"), "original\n").unwrap();
    run(&checkout, &["add", "tracked"]);
    run(&checkout, &["commit", "-m", "initial"]);
    let input = Input {
        repo_key: Some(directory.path().join(".git").to_str().unwrap().to_owned()),
        branch: "test".into(),
        checkout: Some(checkout.clone()),
    };
    (directory, hooks, checkout, input)
}

fn run(checkout: &str, args: &[&str]) {
    let result = Command::new("git")
        .args(["-C", checkout])
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn refresh(input: &Input) -> Focused {
    let Completion::Status(result) = execute(input, Job::Status, &|| false) else {
        panic!("a focused refresh answers with status");
    };
    result.unwrap()
}

fn head(checkout: &str) -> String {
    let result = Command::new("git")
        .args(["-C", checkout, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn assert_head_moves(checkout: &str, message: &str) {
    let before = head(checkout);
    let outcome = commit(
        checkout,
        message,
        Instant::now() + Duration::from_secs(30),
        &|| false,
    )
    .unwrap();
    assert!(outcome.message.contains(message), "{}", outcome.message);
    assert_ne!(head(checkout), before);
}

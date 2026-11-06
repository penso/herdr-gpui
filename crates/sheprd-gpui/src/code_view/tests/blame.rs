//! `b` shows who last changed each line, from Git, beside the file as read.
use super::*;
use std::path::Path;

fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=a@a",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A checkout whose `a.rs` has a committed line and one not committed yet.
fn checkout() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    git(dir.path(), &["init", "-q"]);
    std::fs::write(&path, "fn one() {}\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "First commit"]);
    std::fs::write(&path, "fn one() {}\nfn two() {}\n").unwrap();
    (dir, path)
}

fn open_focused(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    path: &Path,
    line: Option<u32>,
) {
    let target = EditorTarget {
        path: path.to_owned(),
        line,
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_code_view(&target, window, cx)));
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn the_key_shows_and_hides_each_lines_commit(cx: &mut TestAppContext) {
    let (dir, path) = checkout();
    let (view, cx) = window(cx);
    open_focused(&view, cx, &path, Some(1));
    assert!(cx.debug_bounds("code-blame-0").is_none());

    cx.simulate_keystrokes("b");
    cx.run_until_parked();
    draw(cx);

    view.read_with(cx, |view, _| {
        let code = view.code_views.values().next().unwrap();
        let blame = code.blame.shown().unwrap();
        assert_eq!(blame.line(0).unwrap().summary, "First commit");
        assert_eq!(blame.line(0).unwrap().author, "Ada");
        assert!(blame.line(1).unwrap().uncommitted());
    });
    assert!(cx.debug_bounds("code-blame-0").is_some());
    assert!(
        cx.debug_bounds("code-blame-bar").is_some(),
        "the marked line's commit shows in full"
    );

    cx.simulate_keystrokes("b");
    draw(cx);
    assert!(cx.debug_bounds("code-blame-0").is_none());
    assert!(cx.debug_bounds("code-blame-bar").is_none());
    drop(dir);
}

#[gpui::test]
fn the_lines_on_screen_are_those_blamed(cx: &mut TestAppContext) {
    let (dir, path) = checkout();
    let (view, cx) = window(cx);
    open_focused(&view, cx, &path, None);
    // An agent adds a line after the tab read the file.
    std::fs::write(&path, "fn one() {}\nfn two() {}\nfn three() {}\n").unwrap();
    cx.simulate_keystrokes("b");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let code = view.code_views.values().next().unwrap();
        assert_eq!(code.shown().1, Some(3));
        assert_eq!(code.blame.shown().unwrap().len(), 3);
    });
    drop(dir);
}

#[gpui::test]
fn opening_the_file_again_keeps_blame_on_and_current(cx: &mut TestAppContext) {
    let (dir, path) = checkout();
    let (view, cx) = window(cx);
    open_focused(&view, cx, &path, None);
    cx.simulate_keystrokes("b");
    cx.run_until_parked();
    std::fs::write(&path, "fn one() {}\nfn two() {}\nfn three() {}\n").unwrap();
    open_focused(&view, cx, &path, None);
    view.read_with(cx, |view, _| {
        let code = view.code_views.values().next().unwrap();
        assert_eq!(code.blame.shown().unwrap().len(), 3);
    });
    drop(dir);
}

#[gpui::test]
fn a_file_git_does_not_track_says_why_and_turns_blame_off(cx: &mut TestAppContext) {
    let (dir, _) = checkout();
    let path = dir.path().join("new.rs");
    std::fs::write(&path, "fn new() {}\n").unwrap();
    let (view, cx) = window(cx);
    open_focused(&view, cx, &path, None);
    cx.simulate_keystrokes("b");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let code = view.code_views.values().next().unwrap();
        assert!(!code.blame.wanted());
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(
            flash.text.as_ref(),
            crate::blame::Error::Untracked.to_string()
        );
    });
}

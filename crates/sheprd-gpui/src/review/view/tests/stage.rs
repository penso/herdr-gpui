//! A hunk stages from its header: only its lines reach the index, its button
//! says so, and a review that does not show what would be staged offers
//! nothing.
use super::{the, window};
use crate::{
    SheprdWindow,
    git::{Git, Status},
    pull_request::Input,
    review::diff::Scope,
    stage::Span,
};
use gpui::{Entity, VisualTestContext};
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn numbered(edits: &[(u32, &str)]) -> String {
    (1..=30)
        .map(|line| {
            edits
                .iter()
                .find(|(number, _)| *number == line)
                .map_or_else(|| format!("line {line}\n"), |(_, text)| format!("{text}\n"))
        })
        .collect()
}

/// A checkout whose `a.txt` has two hunks not staged yet.
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("a.txt"), numbered(&[])).unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    std::fs::write(
        root.join("a.txt"),
        numbered(&[(3, "\tthree"), (25, "twenty-five")]),
    )
    .unwrap();
    dir
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// Opens the review of `root`, every line read.
fn open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, root: &Path) {
    let input = Input {
        checkout: Some(root.to_str().unwrap().into()),
        repo_key: None,
        branch: "main".into(),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(input, Status::default());
            view.open_review(window, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
    cx.run_until_parked();
    draw(cx);
}

/// The header lines of the first file's hunks.
fn headers(view: &Entity<SheprdWindow>, cx: &VisualTestContext) -> Vec<usize> {
    view.read_with(cx, |view, _| {
        let review = view.reviews.values().next().unwrap();
        let lines = review.loaded().unwrap().diff.files[0].lines().unwrap();
        lines.hunks().to_vec()
    })
}

fn stage(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, header: usize) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = the(view);
            view.stage_review_hunk(id, 0, header, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
}

fn staged(view: &Entity<SheprdWindow>, cx: &VisualTestContext, header: usize) -> Option<bool> {
    view.read_with(cx, |view, _| {
        view.reviews.values().next().unwrap().hunk_staged(0, header)
    })
}

#[test]
fn a_hunks_span_covers_its_changes_not_its_context() {
    let parsed = crate::review::diff::Diff::parse(
        "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,5 +1,5 @@\n one\n two\n-three\n+THREE\n four\n five\n@@ -20,3 +20,2 @@\n x\n-gone\n y\n",
    );
    let lines = parsed.files[0].lines().unwrap();
    let hunks = lines.hunks();
    assert_eq!(lines.hunk_span(hunks[0]), Some(Span { start: 3, end: 4 }));
    assert_eq!(
        lines.hunk_span(hunks[1]),
        Some(Span { start: 21, end: 21 }),
        "a removal is a point before the next line"
    );
    assert_eq!(lines.hunk_span(1), None, "not a header");
}

#[test]
fn a_hunk_with_more_context_keeps_its_span() {
    let parsed = crate::review::diff::Diff::parse(
        "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -5,3 +5,3 @@\n a\n-b\n+B\n c\n",
    );
    let lines = parsed.files[0].lines().unwrap();
    let grown = lines.with_context(0, 3, 3, &["x".to_owned(), "y".to_owned()]);
    assert_eq!(grown.hunk_span(0), Some(Span { start: 6, end: 7 }));
}

#[gpui::test]
fn staging_a_hunk_takes_its_lines_alone_and_the_button_says_so(cx: &mut gpui::TestAppContext) {
    let dir = checkout();
    let root = dir.path();
    let (view, cx) = window(cx, None);
    open(&view, cx, root);
    let hunks = headers(&view, cx);
    assert_eq!(hunks.len(), 2);
    assert_eq!(staged(&view, cx, hunks[0]), Some(false));
    let button = format!("review-stage-hunk-0-{}", hunks[0]);
    assert!(
        cx.debug_bounds(Box::leak(button.into_boxed_str()))
            .is_some()
    );

    stage(&view, cx, hunks[0]);

    // The review showed spaces for the tab; the index keeps the tab.
    assert_eq!(git(root, &["show", ":a.txt"]), numbered(&[(3, "\tthree")]),);
    assert_eq!(staged(&view, cx, hunks[0]), Some(true));
    assert_eq!(staged(&view, cx, hunks[1]), Some(false));
    let word = format!("review-hunk-staged-0-{}", hunks[0]);
    assert!(cx.debug_bounds(Box::leak(word.into_boxed_str())).is_some());
    view.read_with(cx, |view, _| assert!(view.flash.is_none()));
}

#[gpui::test]
fn a_hunk_already_staged_stays_marked_when_the_review_is_read_again(cx: &mut gpui::TestAppContext) {
    let dir = checkout();
    let root = dir.path();
    git(root, &["add", "a.txt"]);
    let (view, cx) = window(cx, None);
    open(&view, cx, root);
    for header in headers(&view, cx) {
        assert_eq!(staged(&view, cx, header), Some(true));
    }
}

#[gpui::test]
fn a_file_changed_since_it_was_read_stages_nothing_and_says_why(cx: &mut gpui::TestAppContext) {
    let dir = checkout();
    let root = dir.path();
    let (view, cx) = window(cx, None);
    open(&view, cx, root);
    let hunks = headers(&view, cx);
    std::fs::write(root.join("a.txt"), numbered(&[(3, "rewritten")])).unwrap();

    stage(&view, cx, hunks[0]);

    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "");
    view.read_with(cx, |view, _| {
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(
            flash.text.as_ref(),
            crate::stage::Error::Changed.to_string()
        );
    });
}

#[gpui::test]
fn an_untracked_file_stages_whole(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("a.txt"), "old\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    std::fs::write(root.join("b.txt"), "new\nlines\n").unwrap();
    let (view, cx) = window(cx, None);
    open(&view, cx, root);
    let hunks = headers(&view, cx);

    stage(&view, cx, hunks[0]);

    assert_eq!(git(root, &["show", ":b.txt"]), "new\nlines\n");
    let hunks = headers(&view, cx);
    assert_eq!(staged(&view, cx, hunks[0]), Some(true));
}

#[gpui::test]
fn a_stage_finishing_after_the_chrome_moved_says_nothing_of_the_new_checkout(
    cx: &mut gpui::TestAppContext,
) {
    let dir = checkout();
    let (view, cx) = window(cx, None);
    open(&view, cx, dir.path());
    let hunks = headers(&view, cx);
    let other = Input {
        checkout: Some("/elsewhere".into()),
        repo_key: None,
        branch: "main".into(),
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = the(view);
            view.stage_review_hunk(id, 0, hunks[0], cx);
            view.git = Git::fixture(other, Status::default());
        })
    });
    cx.run_until_parked();

    assert_ne!(git(dir.path(), &["diff", "--cached", "--name-only"]), "");
    view.read_with(cx, |view, _| {
        assert_eq!(view.git.status().unwrap().staged, 0);
    });
}

#[gpui::test]
fn a_new_file_grown_since_the_review_stages_nothing(cx: &mut gpui::TestAppContext) {
    let dir = checkout();
    let root = dir.path();
    git(root, &["checkout", "-q", "a.txt"]);
    std::fs::write(root.join("b.txt"), "seen\n").unwrap();
    let (view, cx) = window(cx, None);
    open(&view, cx, root);
    let hunks = headers(&view, cx);
    std::fs::write(root.join("b.txt"), "seen\nunseen\n").unwrap();

    stage(&view, cx, hunks[0]);

    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "");
    assert_eq!(staged(&view, cx, hunks[0]), Some(false));
}

#[gpui::test]
fn a_hunk_unstaged_elsewhere_is_offered_again_with_the_same_file_staged(
    cx: &mut gpui::TestAppContext,
) {
    let dir = checkout();
    let root = dir.path();
    let (view, cx) = window(cx, None);
    open(&view, cx, root);
    let hunks = headers(&view, cx);
    stage(&view, cx, hunks[0]);
    stage(&view, cx, hunks[1]);
    assert_eq!(staged(&view, cx, hunks[1]), Some(true));
    let tracked = view.read_with(cx, |view, _| view.git.tracked().cloned().unwrap());
    let seen = Status {
        staged: 1,
        index: 1,
        ..Status::default()
    };
    view.update_in(cx, |view, _, cx| {
        view.git = Git::fixture(tracked.clone(), seen);
        view.follow_review_changes(cx);
    });
    cx.run_until_parked();

    // The file stays staged, with one of its two hunks taken back out.
    git(root, &["restore", "--staged", "a.txt"]);
    std::fs::write(root.join("kept"), numbered(&[(3, "\tthree")])).unwrap();
    let blob = git(root, &["hash-object", "-w", "kept"]);
    std::fs::remove_file(root.join("kept")).unwrap();
    let entry = format!("100644,{},a.txt", blob.trim());
    git(root, &["update-index", "--cacheinfo", &entry]);
    view.update_in(cx, |view, _, cx| {
        view.git = Git::fixture(tracked, Status { index: 2, ..seen });
        view.follow_review_changes(cx);
    });
    cx.run_until_parked();

    assert_eq!(staged(&view, cx, hunks[0]), Some(true));
    assert_eq!(staged(&view, cx, hunks[1]), Some(false));
}

#[gpui::test]
fn a_review_not_showing_what_would_be_staged_offers_nothing(cx: &mut gpui::TestAppContext) {
    let dir = checkout();
    let (view, cx) = window(cx, None);
    open(&view, cx, dir.path());
    let hunks = headers(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = the(view);
            view.toggle_review_whitespace(id, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);
    cx.run_until_parked();
    let hunks_now = headers(&view, cx);
    assert_eq!(staged(&view, cx, hunks_now[0]), None);
    stage(&view, cx, hunks_now[0]);
    assert_eq!(git(dir.path(), &["diff", "--cached", "--name-only"]), "");
    view.update_in(cx, |view, _, cx| {
        let id = the(view);
        view.toggle_review_whitespace(id, cx);
        let review = view.reviews.get_mut(&id).unwrap();
        review.scope = Scope::Branch;
        assert_eq!(review.hunk_staged(0, hunks[0]), None);
    });
}

#[test]
fn the_listing_of_unstaged_changes_names_files_as_git_does() {
    let dir = checkout();
    let root = dir.path();
    std::fs::write(root.join("b c.txt"), "x\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "second"]);
    std::fs::write(root.join("b c.txt"), "y\n").unwrap();
    std::fs::write(root.join("a.txt"), numbered(&[(10, "ten")])).unwrap();
    let unstaged = crate::review::diff::unstaged(root.to_str().unwrap()).unwrap();
    assert!(unstaged["a.txt"].contains(&Span { start: 10, end: 11 }));
    assert_eq!(unstaged["b c.txt"], [Span { start: 1, end: 2 }]);
}

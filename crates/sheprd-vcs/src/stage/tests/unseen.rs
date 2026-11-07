//! Nothing reaches the index that the review did not show: not a line gone
//! since, not a whole file gone since, and not a line added to a new file.
use super::*;

fn refused(result: crate::Result<()>, root: &Path) {
    assert!(matches!(
        result.unwrap_err(),
        crate::Error::Stage(Error::Changed)
    ));
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "");
}

#[test]
fn a_line_removed_since_it_was_shown_stages_nothing() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let text: String = numbered(&[(5, "five")])
        .lines()
        .filter(|line| *line != "line 6")
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(root.join("a.txt"), text).unwrap();

    refused(stage_hunk(&checkout, &hunk(5, "five"), same), root);
}

#[test]
fn a_file_deleted_since_it_was_shown_stages_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("a.txt"), "line 1\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    std::fs::remove_file(root.join("a.txt")).unwrap();

    // The review showed line 1 changed; the file has gone since.
    let result = stage_hunk(root.to_str().unwrap(), &hunk(1, "one"), same);

    refused(result, root);
}

#[test]
fn a_line_shown_that_has_moved_out_of_the_diff_stages_nothing() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    // Line 5's edit was undone and only the removal around it remains.
    std::fs::write(root.join("a.txt"), numbered(&[(6, "six")])).unwrap();
    let mut shown = hunk(5, "five");
    shown.removed.push("line 6".into());
    shown.changes[0].end = 7;

    refused(stage_hunk(&checkout, &shown, same), root);
}

#[test]
fn a_removed_line_shown_once_covers_one_removal() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "a\nsame\nsame\nb\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "repeats"]);
    std::fs::write(root.join("a.txt"), "a\nb\n").unwrap();
    let shown = Hunk {
        path: "a.txt".into(),
        changes: vec![Span { start: 2, end: 2 }],
        lines: HashMap::new(),
        removed: vec!["same".into()],
    };

    refused(stage_hunk(&checkout, &shown, same), root);
}

#[test]
fn a_line_staged_then_edited_again_stages_as_shown() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), numbered(&[(5, "middle")])).unwrap();
    git(root, &["add", "a.txt"]);
    std::fs::write(root.join("a.txt"), numbered(&[(5, "final")])).unwrap();

    // The review, against HEAD, shows line 5 becoming `final`.
    stage_hunk(&checkout, &hunk(5, "final"), same).unwrap();

    assert_eq!(git(root, &["show", ":a.txt"]), numbered(&[(5, "final")]));
}

#[test]
fn a_staged_change_beside_the_hunk_stays_staged() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let extra = numbered(&[(5, "line 5\nextra")]);
    std::fs::write(root.join("a.txt"), &extra).unwrap();
    git(root, &["add", "a.txt"]);
    // Gone from the file again, so a review against HEAD does not show it,
    // and an edit near it, which is what the review shows.
    std::fs::write(root.join("a.txt"), numbered(&[(9, "nine")])).unwrap();

    stage_hunk(&checkout, &hunk(9, "nine"), same).unwrap();

    assert_eq!(
        git(root, &["show", ":a.txt"]),
        numbered(&[(5, "line 5\nextra"), (9, "nine")])
    );
}

#[test]
fn a_hunk_of_several_changes_stages_them_all_and_nothing_between() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let extra = numbered(&[(5, "line 5\nextra")]);
    std::fs::write(root.join("a.txt"), &extra).unwrap();
    git(root, &["add", "a.txt"]);
    let text = numbered(&[(3, "three"), (8, "eight")]);
    std::fs::write(root.join("a.txt"), &text).unwrap();
    let shown = Hunk {
        path: "a.txt".into(),
        changes: vec![Span { start: 3, end: 4 }, Span { start: 8, end: 9 }],
        lines: HashMap::from([(3, "three".to_owned()), (8, "eight".to_owned())]),
        removed: vec!["line 3".into(), "line 8".into()],
    };

    stage_hunk(&checkout, &shown, same).unwrap();

    assert_eq!(
        git(root, &["show", ":a.txt"]),
        numbered(&[(3, "three"), (5, "line 5\nextra"), (8, "eight")])
    );
}

#[test]
fn a_checkout_with_no_commit_yet_stages_a_hunk() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("a.txt"), numbered(&[])).unwrap();
    git(root, &["add", "a.txt"]);
    std::fs::write(root.join("a.txt"), numbered(&[(5, "five")])).unwrap();

    stage_hunk(root.to_str().unwrap(), &hunk(5, "five"), same).unwrap();

    assert_eq!(git(root, &["show", ":a.txt"]), numbered(&[(5, "five")]));
}

#[test]
fn a_new_file_that_grew_since_it_was_shown_stages_nothing() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("new.txt"), "seen\nunseen\n").unwrap();

    refused(
        stage_file(&checkout, &whole("new.txt", &["seen"]), same),
        root,
    );
}

#[test]
fn a_new_file_edited_since_it_was_shown_stages_nothing() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("new.txt"), "edited\n").unwrap();

    refused(
        stage_file(&checkout, &whole("new.txt", &["seen"]), same),
        root,
    );
}

#[test]
fn a_new_file_gone_since_it_was_shown_stages_nothing() {
    let (dir, checkout) = checkout();

    let shown = whole("new.txt", &["seen"]);

    refused(stage_file(&checkout, &shown, same), dir.path());
}

#[cfg(unix)]
#[test]
fn a_changed_link_compares_by_its_target() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::os::unix::fs::symlink("first", root.join("link")).unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "link"]);
    std::fs::remove_file(root.join("link")).unwrap();
    std::os::unix::fs::symlink("second", root.join("link")).unwrap();
    let shown = Hunk {
        path: "link".into(),
        changes: vec![Span { start: 1, end: 2 }],
        lines: HashMap::from([(1, "second".to_owned())]),
        removed: vec!["first".into()],
    };

    stage_hunk(&checkout, &shown, same).unwrap();

    assert_eq!(git(root, &["show", ":link"]), "second");
}

#![allow(clippy::unwrap_used)]
use super::*;
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

mod unseen;

/// The one hunk of the untracked file `path`, holding `lines`.
fn whole(path: &str, lines: &[&str]) -> Hunk {
    let last = u32::try_from(lines.len()).unwrap() + 1;
    Hunk {
        path: path.into(),
        changes: vec![Span {
            start: 1,
            end: last,
        }],
        lines: (1..)
            .zip(lines.iter().map(|line| (*line).to_owned()))
            .collect(),
        removed: Vec::new(),
    }
}

/// A checkout holding `a.txt` of twenty numbered lines.
fn checkout() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("a.txt"), numbered(&[])).unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    let path = root.to_str().unwrap().to_owned();
    (dir, path)
}

/// Lines 1 to 20, each in `edits` replaced by its new text.
fn numbered(edits: &[(u32, &str)]) -> String {
    (1..=20)
        .map(|line| {
            let text = edits
                .iter()
                .find(|(number, _)| *number == line)
                .map_or_else(|| format!("line {line}"), |(_, text)| (*text).to_owned());
            format!("{text}\n")
        })
        .collect()
}

/// The hunk of `a.txt` changing line `line` to `text`, as a review shows it.
fn hunk(line: u32, text: &str) -> Hunk {
    Hunk {
        path: "a.txt".into(),
        changes: vec![Span {
            start: line,
            end: line + 1,
        }],
        lines: HashMap::from([(line, text.to_owned())]),
        removed: vec![format!("line {line}")],
    }
}

fn same(text: &str) -> String {
    text.to_owned()
}

#[test]
fn spans_meet_at_their_ends() {
    let span = |start, end| Span { start, end };
    assert!(span(3, 5).touches(span(5, 5)));
    assert!(span(5, 5).touches(span(3, 5)));
    assert!(span(3, 5).touches(span(4, 4)));
    assert!(!span(3, 5).touches(span(6, 9)));
}

#[test]
fn a_walk_marks_additions_and_the_place_of_removals() {
    let mut walk = Walk::new(10, 4);
    walk.context();
    walk.removed();
    assert_eq!(walk.span(), Some(Span { start: 11, end: 11 }));
    walk.added();
    walk.context();
    assert_eq!(walk.span(), Some(Span { start: 11, end: 12 }));
}

#[test]
fn a_side_with_no_lines_names_the_line_before() {
    let mut walk = Walk::new(4, 0);
    walk.removed();
    assert_eq!(walk.span(), Some(Span { start: 5, end: 5 }));
}

#[test]
fn the_new_side_of_a_header_reads_with_or_without_a_count() {
    assert_eq!(new_side(b"@@ -1,2 +3,4 @@ fn x()\n"), Some((3, 4)));
    assert_eq!(new_side(b"@@ -1 +3 @@\n"), Some((3, 1)));
    assert_eq!(new_side(b"@@ -1,2 +0,0 @@\n"), Some((0, 0)));
    assert_eq!(new_side(b"not a header\n"), None);
    assert_eq!(new_side(b"@@ -1,2 3,4 @@\n"), None);
}

#[test]
fn a_diff_splits_at_its_hunks_and_keeps_every_byte() {
    let diff = b"diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n@@ -9 +9 @@\n-c\n+d";
    let (header, hunks) = split(diff);
    assert_eq!(header, b"diff --git a/a b/a\n--- a/a\n+++ b/a\n");
    assert_eq!(
        hunks,
        [&b"@@ -1 +1 @@\n-a\n+b\n"[..], b"@@ -9 +9 @@\n-c\n+d"]
    );
    assert_eq!(split(b"Binary files differ\n").1.len(), 0);
}

#[test]
fn one_hunk_stages_and_the_others_stay_unstaged() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let edited = numbered(&[(2, "two"), (18, "eighteen")]);
    std::fs::write(root.join("a.txt"), &edited).unwrap();

    stage_hunk(&checkout, &hunk(18, "eighteen"), same).unwrap();

    assert_eq!(
        git(root, &["show", ":a.txt"]),
        numbered(&[(18, "eighteen")]),
        "only the later hunk, though the earlier one shifts nothing it needs"
    );
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), edited);
}

#[test]
fn a_hunk_after_one_left_out_still_applies_when_lines_were_added() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let mut text = String::from("new 1\nnew 2\nnew 3\n");
    text.push_str(&numbered(&[(18, "eighteen")]));
    std::fs::write(root.join("a.txt"), &text).unwrap();

    let mut shown = hunk(21, "eighteen");
    shown.removed = vec!["line 18".into()];
    stage_hunk(&checkout, &shown, same).unwrap();

    assert_eq!(
        git(root, &["show", ":a.txt"]),
        numbered(&[(18, "eighteen")])
    );
}

#[test]
fn a_hunk_of_removed_lines_stages_by_its_place() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let text: String = numbered(&[])
        .lines()
        .filter(|line| *line != "line 10")
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(root.join("a.txt"), &text).unwrap();
    let hunk = Hunk {
        path: "a.txt".into(),
        changes: vec![Span { start: 10, end: 10 }],
        lines: HashMap::new(),
        removed: vec!["line 10".into()],
    };

    stage_hunk(&checkout, &hunk, same).unwrap();

    assert_eq!(git(root, &["show", ":a.txt"]), text);
}

#[test]
fn a_hunk_with_a_missing_final_newline_stages_exactly() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    let text = numbered(&[(20, "twenty")]);
    let text = text.trim_end_matches('\n');
    std::fs::write(root.join("a.txt"), text).unwrap();

    stage_hunk(&checkout, &hunk(20, "twenty"), same).unwrap();

    assert_eq!(git(root, &["show", ":a.txt"]), text);
}

#[test]
fn staging_twice_says_there_is_nothing_left() {
    let (dir, checkout) = checkout();
    std::fs::write(dir.path().join("a.txt"), numbered(&[(5, "five")])).unwrap();
    stage_hunk(&checkout, &hunk(5, "five"), same).unwrap();
    let error = stage_hunk(&checkout, &hunk(5, "five"), same).unwrap_err();
    assert!(matches!(error, crate::Error::Stage(Error::NothingToStage)));
}

#[test]
fn a_line_changed_since_it_was_shown_stages_nothing() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), numbered(&[(5, "rewritten")])).unwrap();

    let error = stage_hunk(&checkout, &hunk(5, "five"), same).unwrap_err();

    assert!(matches!(error, crate::Error::Stage(Error::Changed)));
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "");
}

#[test]
fn a_line_added_beside_what_was_shown_stages_nothing() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(
        root.join("a.txt"),
        numbered(&[(5, "five"), (6, "and something unseen")]),
    )
    .unwrap();

    let error = stage_hunk(&checkout, &hunk(5, "five"), same).unwrap_err();

    assert!(matches!(error, crate::Error::Stage(Error::Changed)));
}

#[test]
fn lines_compare_as_they_were_shown() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), numbered(&[(5, "\tfive")])).unwrap();

    stage_hunk(&checkout, &hunk(5, "    five"), |text| {
        text.replace('\t', "    ")
    })
    .unwrap();

    assert_eq!(
        git(root, &["show", ":a.txt"]),
        numbered(&[(5, "\tfive")]),
        "the tab reaches the index though the review showed spaces"
    );
}

#[test]
fn a_name_with_pathspec_characters_stages_only_itself() {
    let (dir, checkout) = checkout();
    let root = dir.path();
    std::fs::write(root.join("[a]*.txt"), "new\n").unwrap();
    std::fs::write(root.join("a.log"), "other\n").unwrap();

    stage_file(&checkout, &whole("[a]*.txt", &["new"]), same).unwrap();

    assert_eq!(
        git(root, &["diff", "--cached", "--name-only"]).trim(),
        "\"[a]*.txt\"".trim_matches('"')
    );
}

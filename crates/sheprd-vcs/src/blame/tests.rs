#![allow(clippy::unwrap_used)]
use super::*;
use std::process::Command;

const ONE: &str = "1111111111111111111111111111111111111111";
const TWO: &str = "2222222222222222222222222222222222222222";
const NONE: &str = "0000000000000000000000000000000000000000";

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
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

fn details(id: &str, group: &str, author: &str, time: u32, summary: &str) -> String {
    format!(
        "{id} 1 1 {group}\nauthor {author}\nauthor-mail <a@a>\nauthor-time {time}\n\
         author-tz +0000\ncommitter x\nsummary {summary}\nfilename a.rs\n"
    )
}

#[test]
fn a_commit_repeated_keeps_its_details_and_lines_follow_the_file() {
    let text = format!(
        "{}\tfirst\n{TWO} 2 2 1\n{}\tsecond\n{ONE} 3 3\n\tthird\n",
        details(ONE, "1", "Ada", 100, "Start"),
        &details(TWO, "1", "Bob", 200, "Edit")[format!("{TWO} 1 1 1\n").len()..],
    );
    let blame = Blame::parse(text.as_bytes());
    assert_eq!(blame.len(), 3);
    assert_eq!(blame.line(0).unwrap().author, "Ada");
    assert_eq!(blame.line(0).unwrap().id, "11111111");
    assert_eq!(blame.line(1).unwrap().time, 200);
    assert_eq!(blame.line(1).unwrap().summary, "Edit");
    assert_eq!(blame.line(2), blame.line(0));
    assert_eq!(blame.line(3), None);
}

#[test]
fn file_text_never_reads_as_a_header() {
    let text = format!(
        "{}\tauthor Mallory\n{ONE} 2 2\n\t{TWO} 1 1 1\n",
        details(ONE, "2", "Ada", 1, "Start"),
    );
    let blame = Blame::parse(text.as_bytes());
    assert_eq!(blame.len(), 2);
    assert_eq!(blame.line(0).unwrap().author, "Ada");
    assert_eq!(blame.line(1), blame.line(0));
}

#[test]
fn uncommitted_lines_have_no_id() {
    let text = format!(
        "{}\tnew\n",
        details(NONE, "1", "Not Committed Yet", 5, "Version of a.rs")
    );
    let blame = Blame::parse(text.as_bytes());
    assert!(blame.line(0).unwrap().uncommitted());
}

#[test]
fn names_and_summaries_are_cleaned_and_bounded() {
    let long = "x".repeat(MAX_TEXT * 2);
    let text = format!("{}\tline\n", details(ONE, "1", "A\u{202e}da\x1b", 1, &long));
    let blame = Blame::parse(text.as_bytes());
    let commit = blame.line(0).unwrap();
    assert_eq!(commit.author, "Ada");
    assert_eq!(commit.summary.chars().count(), MAX_TEXT);
}

#[test]
fn output_that_is_not_blame_gives_no_lines() {
    assert!(Blame::parse(b"").is_empty());
    assert!(Blame::parse(b"author nobody\n\tline\nnot a header\n").is_empty());
}

#[test]
fn a_run_starts_where_the_commit_changes() {
    let commits = vec![Commit::default(), Commit::default()];
    let blame = Blame::fixture(commits, vec![0, 0, 1, 0]);
    let runs: Vec<bool> = (0..4).map(|line| blame.starts_run(line)).collect();
    assert_eq!(runs, [true, false, true, true]);
}

#[test]
fn a_real_file_blames_committed_and_edited_lines() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("src")).unwrap();
    let path = root.join("src/a b.rs");
    git(root, &["init", "-q"]);
    std::fs::write(&path, "one\ntwo\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "First"]);
    std::fs::write(&path, "one\nchanged\nthree\n").unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    let blame = Blame::read(&path, &contents, &AtomicBool::new(false)).unwrap();

    assert_eq!(blame.len(), 3);
    assert_eq!(blame.line(0).unwrap().author, "Ada");
    assert_eq!(blame.line(0).unwrap().summary, "First");
    assert!(blame.line(1).unwrap().uncommitted());
    assert!(blame.line(2).unwrap().uncommitted());
}

#[test]
fn an_untracked_file_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("a.rs"), "one\n").unwrap();
    let error = Blame::read(&root.join("a.rs"), "one\n", &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, crate::Error::Blame(Error::Untracked)));
}

#[test]
fn a_cancelled_read_stops() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    assert!(Blame::read(&path, "", &AtomicBool::new(true)).is_err());
}

#[test]
fn the_text_read_is_blamed_though_the_file_moved_on() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = root.join("a.rs");
    git(root, &["init", "-q"]);
    std::fs::write(&path, "one\ntwo\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "First"]);
    // Another line arrived at the top after the tab read the file.
    std::fs::write(&path, "zero\none\ntwo\n").unwrap();

    let blame = Blame::read(&path, "one\ntwo\n", &AtomicBool::new(false)).unwrap();

    assert_eq!(blame.len(), 2);
    assert_eq!(blame.line(0).unwrap().summary, "First");
    assert_eq!(blame.line(1).unwrap().summary, "First");
}

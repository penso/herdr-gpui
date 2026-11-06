#![allow(clippy::unwrap_used)]
use super::*;
use std::process::Command;

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
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
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn change_of(listing: &Listing, path: &str) -> Option<Change> {
    listing
        .entries()
        .iter()
        .find(|entry| entry.path == path)
        .unwrap_or_else(|| panic!("{path} is not listed"))
        .change
}

#[test]
fn status_pairs_read_as_the_file_stands_now() {
    for (pair, expected) in [
        (b" M", Some(Change::Modified)),
        (b"M ", Some(Change::Modified)),
        (b"MM", Some(Change::Modified)),
        (b"A ", Some(Change::Added)),
        (b"AM", Some(Change::Added)),
        (b" D", Some(Change::Deleted)),
        (b"D ", Some(Change::Deleted)),
        (b"AD", Some(Change::Deleted)),
        (b"UU", Some(Change::Conflicted)),
        (b"AA", Some(Change::Conflicted)),
        (b"DD", Some(Change::Conflicted)),
        (b"DU", Some(Change::Conflicted)),
        (b"??", Some(Change::Untracked)),
        (b"!!", None),
    ] {
        assert_eq!(change(pair[0], pair[1]), expected, "{pair:?}");
    }
}

#[test]
fn status_records_skip_what_cannot_be_read() {
    let parsed = parse_status(b" M a b.txt\0x\0?? \xff\0A  new\0");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed["a b.txt"], Change::Modified);
    assert_eq!(parsed["new"], Change::Added);
}

#[test]
fn folders_come_before_files_and_stay_together() {
    let mut paths = vec![
        "b.txt",
        "A.txt",
        "src/z.rs",
        "src/a/b.rs",
        "src-old/x",
        "c/d",
    ];
    paths.sort_by(|a, b| tree_order(a, b));
    assert_eq!(
        paths,
        [
            "c/d",
            "src/a/b.rs",
            "src/z.rs",
            "src-old/x",
            "A.txt",
            "b.txt"
        ]
    );
}

#[test]
fn a_name_kept_by_a_staged_deletion_is_listed_once() {
    let listing = Listing::of(Path::new("/r"), b"a\0b\0", b"", b" D a\0D  c\0");
    let paths: Vec<&str> = listing
        .entries()
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, ["a", "b", "c"]);
    assert_eq!(change_of(&listing, "c"), Some(Change::Deleted));
}

#[test]
fn a_status_name_leaving_the_checkout_is_not_listed() {
    let listing = Listing::of(Path::new("/r"), b"a\0", b"", b" D ../secret\0");
    assert_eq!(listing.entries().len(), 1);
}

#[test]
fn a_real_checkout_lists_each_file_with_its_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    write(root, ".gitignore", "ignored.txt\n");
    write(root, "src/kept.rs", "kept\n");
    write(root, "src/edited.rs", "before\n");
    write(root, "gone.rs", "gone\n");
    write(root, "staged-gone.rs", "gone\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "first"]);
    write(root, "src/edited.rs", "after\n");
    write(root, "src/new.rs", "new\n");
    write(root, "added.rs", "added\n");
    write(root, "ignored.txt", "ignored\n");
    git(root, &["add", "added.rs"]);
    git(root, &["rm", "-q", "staged-gone.rs"]);
    std::fs::remove_file(root.join("gone.rs")).unwrap();

    let listing = Listing::read(root, &AtomicBool::new(false)).unwrap();

    assert_eq!(change_of(&listing, "src/kept.rs"), None);
    assert_eq!(change_of(&listing, "src/edited.rs"), Some(Change::Modified));
    assert_eq!(change_of(&listing, "src/new.rs"), Some(Change::Untracked));
    assert_eq!(change_of(&listing, "added.rs"), Some(Change::Added));
    assert_eq!(change_of(&listing, "gone.rs"), Some(Change::Deleted));
    assert_eq!(change_of(&listing, "staged-gone.rs"), Some(Change::Deleted));
    assert!(
        listing
            .entries()
            .iter()
            .all(|entry| entry.path != "ignored.txt")
    );
    assert!(!listing.truncated());
}

#[test]
fn a_cancelled_read_stops() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    assert!(Listing::read(dir.path(), &AtomicBool::new(true)).is_err());
}

//! Host files whose names are as long as a file name can be: the copy's
//! temporary files must not add to that name.

use super::*;
use crate::remote_files::push::push_copy;

/// A 255-byte name, the usual limit for one file name.
fn longest_name() -> String {
    format!("{}.md", "n".repeat(252))
}

#[test]
fn temporary_names_beside_a_copy_stay_short() {
    let local = Path::new("cache").join(longest_name());
    for temporary in [
        cache::part_path(&local),
        cache::kept_path(&local),
        cache::staged_path(&local),
    ] {
        assert_eq!(temporary.parent(), local.parent(), "beside the copy");
        let name = temporary.file_name().unwrap().len();
        assert!(name < 32, "{} is {name} bytes", temporary.display());
    }
}

#[test]
fn a_copy_with_the_longest_name_is_fetched_and_pushed() {
    let host = FakeHost::new();
    let files = {
        let mut files = RemoteFiles::default();
        host.serve(&mut files, notes);
        files
    };
    let path = format!(r"C:\dev\{}", longest_name());
    let done = fetch_fresh(&files.io, files.owner, &remote(&path)).unwrap();
    assert_eq!(std::fs::read(&done.copy.local).unwrap(), NOTES);
    std::fs::write(&done.copy.local, b"# Edited\n").unwrap();
    push_copy(
        &files.io,
        &remote(&path),
        &done.copy.local,
        (Expect::Any, RemoteFileAccess::Keep),
        1024,
    )
    .unwrap();
    assert_eq!(host.received()[0].bytes, b"# Edited\n");
}

/// A host's file may be named like a temporary file. Its copy then shares
/// the folder with temporary names of the same shape, which must never be
/// its own, or a failed write's cleanup would remove the copy.
#[test]
fn a_temporary_name_is_never_the_copy_s_own() {
    let local = Path::new("cache").join(".7.part");
    let mut serials = 6..;
    let mut next = || serials.next().unwrap();
    assert_eq!(
        cache::beside_from(&local, "part", &mut next),
        Path::new("cache").join(".6.part")
    );
    assert_eq!(
        cache::beside_from(&local, "part", &mut next),
        Path::new("cache").join(".8.part"),
        "the copy's own name is skipped"
    );
    assert_eq!(
        cache::beside_from(&local, "kept", || 7),
        Path::new("cache").join(".7.kept"),
        "another extension is another name"
    );
}

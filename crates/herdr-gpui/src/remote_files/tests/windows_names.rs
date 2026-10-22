//! A host's file name that a Windows client cannot store is renamed for the
//! local copy alone; every other client keeps the host's name.

use super::*;
use cache::local_name;

/// Names a POSIX host may hold, with what a Windows client calls its copy.
const RENAMED: &[(&str, &str)] = &[
    ("a<b.txt", "a_b.txt"),
    ("a>b.txt", "a_b.txt"),
    ("a:b.txt", "a_b.txt"),
    ("q\"r.txt", "q_r.txt"),
    ("a/b.txt", "a_b.txt"),
    ("a\\b.txt", "a_b.txt"),
    ("a|b.txt", "a_b.txt"),
    ("why?.txt", "why_.txt"),
    ("*.txt", "_.txt"),
    ("a\u{1}b\u{7f}.txt", "a_b_.txt"),
    ("notes.", "notes_"),
    ("notes ", "notes_"),
    ("con", "_con"),
    ("con.txt", "_con.txt"),
    ("COM1", "_COM1"),
    ("lpt9.log", "_lpt9.log"),
    ("Nul.tar.gz", "_Nul.tar.gz"),
];

/// Names a Windows client stores as they are.
const KEPT: &[&str] = &[
    "console.txt",
    "notes.md",
    "COM0",
    "LPT10.log",
    ".bashrc",
    "\u{e9}t\u{e9} 2024.txt",
];

#[test]
fn a_windows_client_renames_a_name_it_cannot_store() {
    for &(host, local) in RENAMED {
        assert_eq!(local_name(host, true), local, "{host:?}");
    }
}

#[test]
fn a_windows_client_keeps_a_name_it_can_store() {
    for &name in KEPT {
        assert_eq!(local_name(name, true), name, "{name:?}");
    }
}

#[test]
fn other_clients_keep_every_host_name() {
    for name in RENAMED
        .iter()
        .map(|&(host, _)| host)
        .chain(KEPT.iter().copied())
    {
        assert_eq!(local_name(name, false), name, "{name:?}");
    }
}

#[test]
fn the_renamed_copy_still_belongs_to_the_host_s_path() {
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::create(root.path()).unwrap();
    let posix = RemoteShell::Posix;
    let copy = cache.copy_path(Owner(1), "me@devbox", "/home/me/a:b.txt", posix);
    let expected = if cfg!(windows) { "a_b.txt" } else { "a:b.txt" };
    assert_eq!(copy.file_name().unwrap(), expected);
    assert_eq!(
        basename("/home/me/a:b.txt", posix),
        "a:b.txt",
        "the host's name"
    );
}

#[cfg(windows)]
#[test]
fn a_windows_client_renames_a_posix_name_holding_a_backslash() {
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::create(root.path()).unwrap();
    let copy = cache.copy_path(
        Owner(1),
        "me@devbox",
        "/srv/foo\\bar.txt",
        RemoteShell::Posix,
    );
    assert_eq!(copy.file_name().unwrap(), "foo_bar.txt");
}

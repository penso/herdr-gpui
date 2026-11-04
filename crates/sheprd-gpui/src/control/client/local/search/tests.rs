#![allow(clippy::unwrap_used)]
use super::*;

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
}

/// A checkout with a few definitions, tracked and not.
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub struct Workspace {}\n\npub fn open_workspace() {}\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.py"), "def main():\n    pass\n").unwrap();
    git(root, &["add", "src/lib.rs"]);
    dir
}

#[test]
fn symbols_and_files_are_found_in_the_checkout() {
    let dir = checkout();
    let index = Index::build(dir.path(), &AtomicBool::new(false)).unwrap();
    let symbols = find(&index, false, "workspace", 20);
    assert_eq!(
        symbols,
        [
            Found {
                path: "src/lib.rs".into(),
                line: Some(1),
                name: Some("Workspace".into()),
                kind: Some(code_index::Kind::Type.label()),
            },
            Found {
                path: "src/lib.rs".into(),
                line: Some(3),
                name: Some("open_workspace".into()),
                kind: Some(code_index::Kind::Function.label()),
            },
        ]
    );
    assert_eq!(find(&index, false, "workspace", 1).len(), 1);
    let files = find(&index, true, "main:2", 20);
    assert_eq!(
        files,
        [Found {
            path: "src/main.py".into(),
            line: Some(2),
            name: None,
            kind: None,
        }]
    );
    assert_eq!(text(&files), "src/main.py:2");
    assert_eq!(
        text(&symbols[1..]),
        format!(
            "src/lib.rs:3  open_workspace ({})",
            code_index::Kind::Function.label()
        )
    );
}

#[test]
fn a_search_from_a_subdirectory_covers_the_whole_checkout() {
    let dir = checkout();
    let output = search_in(&dir.path().join("src"), true, "lib", 5).unwrap();
    assert_eq!(output.status, 0);
    assert_eq!(output.text, "src/lib.rs");
    assert_eq!(output.json["matches"][0]["path"], "src/lib.rs");
    let none = search_in(dir.path(), false, "zzzqqq", 5).unwrap();
    assert_eq!(none.status, crate::control::client::exit::NOTHING);
}

#[test]
fn a_directory_outside_any_checkout_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(search_in(dir.path(), true, "x", 5).is_err());
}

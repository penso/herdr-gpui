#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn words_follow_shell_quoting() {
    assert_eq!(
        words(r#"/a/My\ File.png '/b/two words.txt' "/c/q\"x".md"#, true).unwrap(),
        ["/a/My File.png", "/b/two words.txt", "/c/q\"x.md"]
    );
    assert!(words("'/unclosed", true).is_none());
}

#[test]
fn windows_paths_keep_their_backslashes() {
    assert_eq!(
        words(r#"C:\shots\a.png "C:\My Files\b.txt""#, false).unwrap(),
        [r"C:\shots\a.png", r"C:\My Files\b.txt"]
    );
}

/// The path as the platform's terminals paste a dropped file.
fn pasted(path: &Path) -> String {
    let path = path.display().to_string();
    if ESCAPES {
        path.replace(' ', "\\ ")
    } else {
        format!("\"{path}\"")
    }
}

#[test]
fn only_a_paste_of_existing_files_is_a_drop() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("my notes.md");
    std::fs::write(&file, "x").unwrap();
    let escaped = pasted(&file);
    assert_eq!(
        files(&format!("{escaped} ")).unwrap(),
        [file.display().to_string()]
    );
    assert!(files("hello world").is_none());
    assert!(files(&format!("{escaped} and more")).is_none());
    assert!(
        files(&dir.path().display().to_string()).is_none(),
        "folders stay text"
    );
    assert!(files("").is_none());
}

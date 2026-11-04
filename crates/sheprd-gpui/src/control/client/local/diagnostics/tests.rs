#![allow(clippy::unwrap_used)]
use super::*;

mod crashes;

fn redactor() -> Redactor {
    Redactor::new(Some(Path::new("/Users/someone")))
}

#[test]
fn the_home_directory_reads_as_a_tilde() {
    assert_eq!(
        redactor().redact("local: /Users/someone/.config/herdr/config-gpui.local.toml\n"),
        "local: ~/.config/herdr/config-gpui.local.toml\n"
    );
    // Without a usable home, paths are kept.
    assert_eq!(
        Redactor::new(Some(Path::new("/"))).redact("/etc/x"),
        "/etc/x"
    );
}

#[test]
fn settings_that_name_a_secret_keep_only_their_key() {
    let text = "[coder]\noauth_client_secret = \"abc def\"\nurl = \"https://coder.example\"\ngithub.token=\"x\"\ntheme = \"Nord\"";
    assert_eq!(
        redactor().redact(text),
        "[coder]\noauth_client_secret = \"[redacted]\"\nurl = \"https://coder.example\"\ngithub.token= \"[redacted]\"\ntheme = \"Nord\""
    );
}

#[test]
fn log_fields_and_words_that_look_like_credentials_are_replaced() {
    let redactor = redactor();
    for (text, expected) in [
        (
            "INFO x request token=\"abc\" pane=w_1",
            "INFO x request token=[redacted] pane=w_1",
        ),
        (
            "WARN x api_key=\"two words\" kept",
            "WARN x api_key=[redacted] [redacted] kept",
        ),
        ("password: hunter2 later", "password: [redacted] later"),
        (
            "Authorization Bearer abc.def.ghi done",
            "Authorization Bearer [redacted] done",
        ),
        (
            "pushed with ghp_0123456789abcdefABCDEF0123",
            "pushed with [redacted]",
        ),
        (
            "key 9f8e7d6c5b4a39281706f5e4d3c2b1a0ffeeddcc",
            "key [redacted]",
        ),
        (
            "build branch=worktree-agent-ac102c3bf717d7bb5",
            "build branch=worktree-agent-ac102c3bf717d7bb5",
        ),
        (
            "aws wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "aws [redacted]",
        ),
        (
            "id=550e8400e29b41d4a716446655440000 next",
            "[redacted] next",
        ),
        (
            "clone https://user:pa55@github.com/a/b.git",
            "clone https://[redacted]@github.com/a/b.git",
        ),
        (
            "path /Users/someone/code/a-very-long-directory-name-with-digits-0123456789",
            "path ~/code/a-very-long-directory-name-with-digits-0123456789",
        ),
        (
            "[2025-09-26 15:03:45] INFO plain",
            "[2025-09-26 15:03:45] INFO plain",
        ),
    ] {
        assert_eq!(redactor.redact(text), expected, "{text}");
    }
}

#[test]
fn a_bundle_never_replaces_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bundle.txt");
    write_new(&path, "first").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
    assert!(matches!(
        write_new(&path, "second"),
        Err(crate::Error::BundleExists { .. })
    ));
    assert!(matches!(
        diagnostics(&path),
        Err(crate::Error::BundleExists { .. })
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
    // Nothing is left behind beside it.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn overrides_are_summarized_and_never_quoted() {
    let file = "theme = \"Nord\"\n[terminal]\nsize = 14\n[github]\nprofile = { token = \"short\" }\nnote = \"hunter2\"\ntoken_mode = \"x\"\nhosts = [\"a\", \"b\"]\n";
    assert_eq!(
        summary(file),
        "github.hosts = <list of 2>\ngithub.note = <text, 7 characters>\ngithub.profile.token = <text, 5 characters>\ngithub.token_mode = <text, 1 characters>\nterminal.size = 14\ntheme = \"Nord\"\n"
    );
    for leaked in ["short", "hunter2"] {
        assert!(!summary(file).contains(leaked), "{leaked}");
    }
    // A parser error would quote the file, so none is shown.
    assert_eq!(summary("token = \"oops"), "(does not parse as TOML)\n");
}

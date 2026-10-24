#![allow(clippy::unwrap_used)]

use super::format_typed;
use crate::worktree::{branch_for_name, validate_branch};

/// `keys` typed one at a time at the end of an empty field.
fn typed(keys: &str) -> String {
    let mut text = String::new();
    for key in keys.chars() {
        let insert = format_typed(&text, &key.to_string(), "");
        text.push_str(&insert);
    }
    text
}

/// `text` pasted into an empty field.
fn pasted(text: &str) -> String {
    format_typed("", text, "")
}

/// Whether Git itself takes `branch` as a branch name.
fn git_accepts(branch: &str) -> bool {
    std::process::Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn spaces_and_refused_characters_become_one_dash() {
    for (keys, branch) in [
        ("Batch Jobs UI", "Batch-Jobs-UI"),
        ("fix  login", "fix-login"),
        ("Fix - login", "Fix-login"),
        ("a~b^c:d?e*f[g\\h", "a-b-c-d-e-f-g-h"),
        ("a\tb\nc\u{7f}d", "a-b-cd"),
        ("tab\u{a0}space", "tab-space"),
    ] {
        assert_eq!(typed(keys), branch, "typed {keys:?}");
        assert_eq!(pasted(keys), branch, "pasted {keys:?}");
    }
}

#[test]
fn pasted_text_loses_the_whitespace_around_it() {
    assert_eq!(pasted(" Batch Jobs UI\n"), "Batch-Jobs-UI");
    assert_eq!(pasted("feat/login\r\n"), "feat/login");
    assert_eq!(pasted("Why does it crash?"), "Why-does-it-crash");
    assert_eq!(format_typed("fix", " login ", ""), "-login");
    // A space typed on its own is the dash the next word follows.
    assert_eq!(format_typed("fix", " ", ""), "-");
}

#[test]
fn what_git_accepts_is_kept_as_typed() {
    for keys in [
        "feat/login",
        "matt/batch-jobs-table-layout-updates",
        "Fix.v2",
        "release/1.2.3",
        "user@host",
        "a./b",
        "a}b{c",
        "\u{65e5}\u{672c}\u{8a9e}/\u{30d6}\u{30e9}\u{30f3}\u{30c1}",
    ] {
        assert_eq!(typed(keys), keys);
        assert_eq!(pasted(keys), keys);
        assert!(git_accepts(keys), "{keys}");
    }
}

/// The sequences `git check-ref-format` refuses never form, whether they are
/// typed or pasted.
#[test]
fn ref_format_sequences_are_kept_out() {
    for (keys, branch) in [
        // `..` anywhere.
        ("a..b", "a.b"),
        ("a...b", "a.b"),
        // `@{` anywhere.
        ("a@{b", "a@-b"),
        ("@{upstream}", "@-upstream}"),
        // A leading dash, dot or slash.
        ("-x", "x"),
        ("--x", "x"),
        (".hidden", "hidden"),
        ("/x", "x"),
        // Consecutive slashes, and a component starting with a dot.
        ("feat//x", "feat/x"),
        ("feat/./x", "feat/x"),
        ("a/.b", "a/b"),
        // A `.lock` component closed by a slash.
        ("deps.lock/x", "deps.lockx"),
        ("a.lock//b", "a.lockb"),
        // A dash beside another, or a separator beside a slash.
        ("a--b", "a-b"),
        ("feat/ x", "feat/x"),
    ] {
        assert_eq!(typed(keys), branch, "typed {keys:?}");
        assert_eq!(pasted(keys), branch, "pasted {keys:?}");
        assert!(validate_branch(branch).is_ok(), "{branch}");
        assert!(git_accepts(branch), "{branch}");
    }
}

/// An ending every name passes through while it is typed stays, so the next
/// key can finish it; the submit check still refuses it as it is.
#[test]
fn an_ending_more_typing_can_finish_is_left_for_the_submit_check() {
    for keys in ["fix.", "feat/", "deps.lock"] {
        assert_eq!(typed(keys), keys);
        assert!(validate_branch(keys).is_err(), "{keys}");
        let finished = typed(&format!("{keys}x"));
        assert!(validate_branch(&finished).is_ok(), "{finished}");
        assert!(git_accepts(&finished), "{finished}");
    }
}

/// Typing between two characters only changes what is typed: an insertion
/// that would form a refused sequence with the text after it is cut back
/// until it does not, and the text around it is never touched.
#[test]
fn an_insertion_never_rewrites_the_text_around_it() {
    for ((before, inserted, after), kept) in [
        (("feat", "/", ".x"), ""),
        (("feat", "/", "/x"), ""),
        (("a.", ".", "b"), ""),
        (("a", "@", "{b"), ""),
        (("a", "-", "-b"), ""),
        (("a", " ", "-b"), ""),
        (("x.lo", "ck", "/b"), "c"),
        (("feat", "/log", "in"), "/log"),
        (("feat/", "Login v2", ""), "Login-v2"),
        (("Login", " v2", "-fix"), "-v2"),
        (("", "-", "x"), ""),
        (("", "x", "-y"), "x"),
    ] {
        assert_eq!(
            format_typed(before, inserted, after),
            kept,
            "{before:?} + {inserted:?} + {after:?}"
        );
    }
}

/// A name proposes `worktree/<slug>`: lowercase ASCII letters and digits,
/// every other run one dash, so whatever is typed as the name gives a branch
/// Git accepts and a folder named the same.
#[test]
fn a_name_proposes_a_branch_git_accepts() {
    for (name, branch) in [
        ("Batch Jobs UI", "worktree/batch-jobs-ui"),
        ("a..b", "worktree/a-b"),
        ("@{upstream}", "worktree/upstream"),
        ("deps.lock", "worktree/deps-lock"),
        ("-flag", "worktree/flag"),
        ("feat//login", "worktree/feat-login"),
        ("Fix: crash on ~/.config", "worktree/fix-crash-on-config"),
    ] {
        assert_eq!(branch_for_name(name).as_deref(), Some(branch), "{name:?}");
        assert!(git_accepts(branch), "{branch}");
    }
}

#![allow(clippy::unwrap_used)]

use super::{SETTLED, format_typed};
use crate::worktree::{branch_for_name, git_accepts, validate_branch};

/// The edit replacing what lies between `before` and `after`, as the text
/// inserted and the whole text that then follows it.
fn format_edit(before: &str, inserted: &str, after: &str) -> (String, String) {
    let edit = super::format_edit(before, inserted, after);
    (
        edit.inserted,
        format!("{}{}", edit.after, &after[edit.replaced..]),
    )
}

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
            format_edit(before, inserted, after),
            (kept.to_owned(), after.to_owned()),
            "{before:?} + {inserted:?} + {after:?}"
        );
    }
}

/// A deletion or cut inserts nothing to cut back, so where the text either
/// side of it would meet in a sequence Git refuses, the text after the caret
/// gives way, only as far as it must, and the result is a branch Git takes.
#[test]
fn a_deletion_never_joins_a_refused_sequence() {
    for ((before, after), rest) in [
        // `x` deleted from `a.x.b`, which would leave `a..b`.
        (("a.", ".b"), "b"),
        (("feat/", "/login"), "login"),
        (("a-", "-b"), "b"),
        (("a@", "{b"), "-b"),
        // A component, or the name, would start with a dot, dash or slash.
        (("feat/", ".x"), "x"),
        (("", "-b"), "b"),
        (("", ".b"), "b"),
        (("", "/b"), "b"),
        (("", "-.b"), "b"),
        // A component now closed after `.lock`.
        (("deps.lock", "/x"), "x"),
        (("deps.lo", "ck/x"), "ckx"),
        // Nothing refused, so nothing moves.
        (("a.", "b.c"), "b.c"),
        (("feat/", "log-in"), "log-in"),
        (("", "feat/login"), "feat/login"),
    ] {
        assert_eq!(
            format_edit(before, "", after),
            (String::new(), rest.to_owned()),
            "{before:?} + {after:?}"
        );
        let branch = format!("{before}{rest}");
        assert!(validate_branch(&branch).is_ok(), "{branch}");
        assert!(git_accepts(&branch), "{branch}");
    }
}

/// Typing over a selection is an insertion and a deletion at once: the
/// insertion is cut back to join the text after it, and that text gives way
/// only where the deletion alone joined a refused sequence.
#[test]
fn replacing_a_selection_joins_both_sides() {
    for ((before, inserted, after), edit) in [
        (("a.", "y", ".b"), ("y", ".b")),
        (("a.", " ", ".b"), ("-", ".b")),
        (("a.", "y.", ".b"), ("y", ".b")),
        (("", "x", "-b"), ("x", "-b")),
        (("feat/", "Log in", "/x"), ("Log-in", "/x")),
    ] {
        let (inserted_kept, rest) = edit;
        assert_eq!(
            format_edit(before, inserted, after),
            (inserted_kept.to_owned(), rest.to_owned()),
            "{before:?} + {inserted:?} + {after:?}"
        );
        let branch = format!("{before}{inserted_kept}{rest}");
        assert!(git_accepts(&branch), "{branch}");
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

/// A deletion's join stops rewriting once the text after it reads as it
/// did: after a few unchanged characters, or an unchanged slash.
#[test]
fn a_deletion_rewrites_only_as_far_as_its_join_reaches() {
    let tail = "x".repeat(SETTLED + 2) + "--y";
    let edit = super::format_edit("a.", "", &format!(".{tail}"));
    assert_eq!(edit.after, "x".repeat(SETTLED));
    assert_eq!(edit.replaced, SETTLED + 1);

    let edit = super::format_edit("feat/", "", "/b/--c");
    assert_eq!((edit.after.as_str(), edit.replaced), ("b/", 3));

    let edit = super::format_edit("a", "b", "cdefgh");
    assert_eq!((edit.inserted.as_str(), edit.replaced), ("b", SETTLED));
    assert_eq!(edit.after, "cdefg");
}

use super::*;

#[test]
fn typed_notes_keep_line_breaks_and_lose_controls() {
    let text = "1. On `a`\n   Note: fix \x1b[201~\rrm -rf ~\x07\u{202e}done\t!\n";
    assert_eq!(
        typable(text),
        "1. On `a`\n   Note: fix [201~rm -rf ~done!\n"
    );
}

#[test]
fn clean_notes_pass_unchanged() {
    let text = "Review notes on your changes in ~/repo.\n\n1. On `src/a.rs:3` (added line)\n   Note: é ✓\n";
    assert_eq!(typable(text), text);
}

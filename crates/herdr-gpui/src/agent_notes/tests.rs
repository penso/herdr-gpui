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

#[test]
fn send_delivers_new_notes_or_all_again() {
    assert_eq!(round([false, false]), [0, 1]);
    assert_eq!(round([true, false, true, false]), [1, 3]);
    assert_eq!(round([true, true]), [0, 1]);
    assert!(round([]).is_empty());
    assert_eq!(send_label(2, 2), "Send to agent");
    assert_eq!(send_label(3, 1), "Send 1 new");
    assert_eq!(send_label(3, 0), "Resend all");
}

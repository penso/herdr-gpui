use super::*;

#[test]
fn previous_start_skips_spaces_and_punctuation_before_the_word() {
    let text = "git commit -m  foo_bar";
    assert_eq!(previous_start(text, text.len()), 15);
    assert_eq!(previous_start(text, 15), 12);
    assert_eq!(previous_start(text, 12), 4);
    assert_eq!(previous_start(text, 6), 4);
    assert_eq!(previous_start(text, 4), 0);
    assert_eq!(previous_start(text, 0), 0);
    assert_eq!(previous_start("  -- ", 5), 0);
}

#[test]
fn next_end_skips_spaces_and_punctuation_after_the_word() {
    let text = "git commit -m  foo_bar";
    assert_eq!(next_end(text, 0), 3);
    assert_eq!(next_end(text, 3), 10);
    assert_eq!(next_end(text, 5), 10);
    assert_eq!(next_end(text, 10), 13);
    assert_eq!(next_end(text, 13), text.len());
    assert_eq!(next_end(text, text.len()), text.len());
    assert_eq!(next_end("a --  ", 1), 6);
}

#[test]
fn boundaries_never_split_a_character() {
    let text = "caf\u{e9} \u{1f600} na\u{ef}ve \u{65e5}\u{672c}";
    let mut offset = text.len();
    while offset > 0 {
        offset = previous_start(text, offset);
        assert!(text.is_char_boundary(offset));
    }
    while offset < text.len() {
        offset = next_end(text, offset);
        assert!(text.is_char_boundary(offset));
    }
    assert_eq!(previous_start(text, 5), 0);
    assert_eq!(next_end(text, 0), 5);
}

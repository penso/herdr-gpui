use super::{drag, frame, surface, text};

#[test]
fn a_row_that_fills_the_pane_is_copied_as_one_line() {
    let mut s = surface("");
    s.frame = frame("abcdefghijKLMNOPQRST", 10, 2);
    assert_eq!(
        text(&s, &drag(&s, (1., 1.), (96., 21.))),
        "abcdefghijKLMNOPQRST"
    );
}

#[test]
fn a_short_line_is_still_copied_with_a_newline() {
    let mut s = surface("");
    s.frame = frame("hello     world     ", 10, 2);
    assert_eq!(text(&s, &drag(&s, (1., 1.), (96., 21.))), "hello\nworld");
}

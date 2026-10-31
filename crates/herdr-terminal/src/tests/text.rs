use super::*;

#[test]
fn plain_text_lands_in_rows() {
    let frame = fed(10, 3, b"hello\r\nworld").frame();
    assert_eq!((frame.width, frame.height), (10, 3));
    assert_eq!(frame.cells.len(), 30);
    assert_eq!(line(&frame, 0), "hello");
    assert_eq!(line(&frame, 1), "world");
    assert_eq!(line(&frame, 2), "");
    assert_eq!(cell(&frame, 7, 0).symbol, " ");
    assert!(frame.graphics.is_empty());
    frame.validate().unwrap();
}

#[test]
fn long_lines_wrap() {
    let frame = fed(4, 3, b"abcdefghij").frame();
    assert_eq!(line(&frame, 0), "abcd");
    assert_eq!(line(&frame, 1), "efgh");
    assert_eq!(line(&frame, 2), "ij");
}

#[test]
fn wide_characters_skip_their_second_cell() {
    let frame = fed(6, 1, "a界b".as_bytes()).frame();
    assert_eq!(cell(&frame, 1, 0).symbol, "界");
    assert!(!cell(&frame, 1, 0).skip);
    let spacer = cell(&frame, 2, 0);
    assert!(spacer.skip);
    assert_eq!(spacer.symbol, "");
    assert_eq!(cell(&frame, 3, 0).symbol, "b");
}

#[test]
fn a_wide_character_that_does_not_fit_wraps_leaving_a_blank() {
    let frame = fed(3, 2, "ab界".as_bytes()).frame();
    assert_eq!(cell(&frame, 2, 0).symbol, " ");
    assert!(!cell(&frame, 2, 0).skip);
    assert_eq!(cell(&frame, 0, 1).symbol, "界");
    assert!(cell(&frame, 1, 1).skip);
}

#[test]
fn combining_characters_join_their_base() {
    let frame = fed(5, 1, "e\u{301}x".as_bytes()).frame();
    assert_eq!(cell(&frame, 0, 0).symbol, "e\u{301}");
    assert_eq!(cell(&frame, 1, 0).symbol, "x");
}

#[test]
fn tabs_show_as_blanks() {
    let frame = fed(12, 1, b"a\tb").frame();
    assert_eq!(cell(&frame, 1, 0).symbol, " ");
    assert_eq!(cell(&frame, 8, 0).symbol, "b");
}

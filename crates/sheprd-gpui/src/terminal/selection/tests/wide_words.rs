use super::*;

#[test]
fn a_double_click_takes_a_word_whose_continuations_lack_the_skip_flag() {
    let mut s = row("ls /tmp/测 试 .md now");
    for column in [8., 9., 10., 11., 14.] {
        assert_eq!(pick(&s, column, 2), "/tmp/测试.md");
    }
    for index in [9, 11] {
        s.frame.cells[index].symbol.clear();
    }
    for column in [8., 9., 10., 11., 14.] {
        assert_eq!(pick(&s, column, 2), "/tmp/测试.md");
    }
}

#[test]
fn a_double_click_takes_a_whole_hyperlink_over_bare_continuations() {
    let mut s = row("文 档  x");
    s.frame.hyperlinks = vec!["https://example.com".into()];
    s.frame.cells[0].hyperlink = Some(0);
    s.frame.cells[2].hyperlink = Some(0);
    for column in [0., 1., 2., 3.] {
        assert_eq!(pick(&s, column, 2), "文档");
    }
}

#[test]
fn text_after_a_wide_symbol_in_a_narrow_cell_is_copied() {
    let mut s = row("!a b");
    s.frame.cells[0].symbol = "\u{26a0}\u{fe0f}".into();
    assert_eq!(pick(&s, 1., 3), "\u{26a0}\u{fe0f}a b");
}

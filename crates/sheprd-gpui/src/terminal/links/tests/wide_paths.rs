//! Wide glyphs in what a link reads. The surface does not mark a wide glyph's
//! continuation cell one way: Herdr 1 sends an empty symbol without the skip
//! flag, herdr-core sets the flag, and a blank may stand there too.

use super::*;
use unicode_width::UnicodeWidthStr;

/// Each continuation cell a frame may hold, as its symbol and skip flag.
const CONTINUATIONS: [(&str, bool); 3] = [("", false), (" ", false), ("", true)];

/// `text` printed from the first cell of a frame, each wide glyph followed by
/// a `continuation` cell, going on at the next row's first column.
fn printed(text: &str, width: u16, height: u16, continuation: (&str, bool)) -> FrameData {
    let mut printed = frame("", width, height);
    let mut cells = printed.cells.iter_mut();
    for c in text.chars() {
        let symbol = c.to_string();
        let wide = symbol.width() == 2;
        cells.next().unwrap().symbol = symbol;
        if wide {
            let cell = cells.next().unwrap();
            cell.symbol = continuation.0.into();
            cell.skip = continuation.1;
        }
    }
    printed
}

fn read(frame: &FrameData, column: u16, row: u16) -> Option<RowLink> {
    frame_link(frame, column, row, 0..frame.width, 0..frame.height)
}

fn bare(path: &str, line: Option<u32>) -> RowTarget {
    RowTarget::Path {
        path: path.into(),
        line,
        explicit: false,
    }
}

#[test]
fn a_path_with_wide_characters_reads_whole_from_any_of_its_cells() {
    for continuation in CONTINUATIONS {
        let frame = printed("/tmp/cjk/测试文档.md", 60, 1, continuation);
        for column in 0..20 {
            let link = read(&frame, column, 0).unwrap();
            assert_eq!(link.target, bare("/tmp/cjk/测试文档.md", None), "{column}");
            assert_eq!(link.columns, 0..20, "{column}");
        }
    }
}

#[test]
fn a_location_after_wide_characters_names_the_line() {
    for continuation in CONTINUATIONS {
        let frame = printed("see docs/方案.md:12:5 ok", 60, 1, continuation);
        let link = read(&frame, 9, 0).unwrap();
        assert_eq!(link.target, bare("docs/方案.md", Some(12)));
        assert_eq!(link.columns, 4..21);
    }
}

#[test]
fn a_real_space_after_a_wide_character_still_ends_the_path() {
    for continuation in CONTINUATIONS {
        let frame = printed("/tmp/文 next", 60, 1, continuation);
        for column in [5, 6] {
            let link = read(&frame, column, 0).unwrap();
            assert_eq!(link.target, bare("/tmp/文", None));
            assert_eq!(link.columns, 0..7);
        }
        assert_eq!(read(&frame, 7, 0), None);
        assert_eq!(read(&frame, 8, 0).unwrap().target, bare("next", None));
    }
}

#[test]
fn a_backslash_after_a_wide_character_still_escapes_the_blank() {
    for continuation in CONTINUATIONS {
        let frame = printed("/tmp/界\\ 文.md", 60, 1, continuation);
        assert_eq!(
            read(&frame, 9, 0).unwrap().target,
            bare("/tmp/界 文.md", None)
        );
    }
}

#[test]
fn a_wide_character_filling_the_row_wraps_with_the_path() {
    // `ls /tmp/资料/a.txt`, the first wide glyph in the last two columns.
    for continuation in CONTINUATIONS {
        let frame = printed("ls /tmp/资料/a.txt", 10, 2, continuation);
        for (column, row) in [(3, 0), (8, 0), (9, 0), (0, 1), (1, 1), (7, 1)] {
            let link = read(&frame, column, row).unwrap();
            assert_eq!(link.target, bare("/tmp/资料/a.txt", None));
            assert_eq!(
                link.rows().collect::<Vec<_>>(),
                vec![(0, 3..10), (1, 0..8)],
                "({column}, {row})"
            );
        }
    }
}

#[test]
fn a_web_address_with_wide_characters_covers_them_all() {
    for continuation in CONTINUATIONS {
        let frame = printed("see https://example.com/文档 ok", 60, 1, continuation);
        let link = read(&frame, 25, 0).unwrap();
        assert!(matches!(link.target, RowTarget::Web(_)), "{link:?}");
        assert_eq!(link.columns, 4..28);
    }
}

#[test]
fn a_hyperlink_covers_continuations_that_do_not_carry_it() {
    for continuation in CONTINUATIONS {
        let mut frame = printed("文档 x", 60, 1, continuation);
        frame.hyperlinks = vec!["https://example.com/".into()];
        frame.cells[0].hyperlink = Some(0);
        frame.cells[2].hyperlink = Some(0);
        for column in 0..4 {
            let link = read(&frame, column, 0).unwrap();
            assert_eq!(link.target, RowTarget::Web("https://example.com/".into()));
            assert_eq!(link.columns, 0..4, "{column}");
        }
    }
}

#[test]
fn text_after_a_wide_symbol_in_a_narrow_cell_stays_in_the_path() {
    // Herdr keeps an emoji presentation sequence in one column when the
    // terminal did: the cell after it is the path's next character.
    let mut narrow = frame("/tmp/!a.md", 60, 1);
    narrow.cells[5].symbol = "\u{26a0}\u{fe0f}".into();
    assert_eq!(narrow.cells[5].symbol.width(), 2);
    let link = read(&narrow, 6, 0).unwrap();
    assert_eq!(link.target, bare("/tmp/\u{26a0}\u{fe0f}a.md", None));
    assert_eq!(link.columns, 0..10);
}

/// An emoji presentation sequence: wide as a whole, narrow as its base.
const WARNING: &str = "\u{26a0}\u{fe0f}";

#[test]
fn a_real_space_after_a_wide_symbol_in_a_narrow_cell_ends_the_link() {
    let mut narrow = frame("/tmp/! next", 60, 1);
    narrow.cells[5].symbol = WARNING.into();
    let link = read(&narrow, 2, 0).unwrap();
    assert_eq!(link.target, bare(&format!("/tmp/{WARNING}"), None));
    assert_eq!(link.columns, 0..6);
    assert_eq!(read(&narrow, 6, 0), None);
    assert_eq!(read(&narrow, 7, 0).unwrap().target, bare("next", None));

    let mut web = frame("https://example.com/! next", 60, 1);
    web.cells[20].symbol = WARNING.into();
    assert_eq!(read(&web, 2, 0).unwrap().columns, 0..21);
}

#[test]
fn an_empty_cell_still_continues_a_wide_symbol_of_two_columns() {
    let mut wide = frame("/tmp/!!a.md", 60, 1);
    wide.cells[5].symbol = WARNING.into();
    wide.cells[6].symbol.clear();
    let link = read(&wide, 6, 0).unwrap();
    assert_eq!(link.target, bare(&format!("/tmp/{WARNING}a.md"), None));
    assert_eq!(link.columns, 0..11);
}

#[test]
fn a_long_grapheme_keeps_its_width_from_its_first_bytes() {
    // Far more combining marks than a width is read from.
    let symbol = format!("界{}", "\u{301}".repeat(200));
    let mut long = frame("/tmp/! a.md", 60, 1);
    long.cells[5].symbol = symbol.clone();
    assert_eq!(
        read(&long, 6, 0).unwrap().target,
        bare(&format!("/tmp/{symbol}a.md"), None)
    );
}

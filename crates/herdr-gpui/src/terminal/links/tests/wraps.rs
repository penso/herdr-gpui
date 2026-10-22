//! Links that run to a row's edge: complete there, or continued on the rows
//! below by a terminal's own wrap or an application's word wrap.
use super::*;

/// Reads as a client whose daemon resolves links, which leaves an address
/// wrapping raw onto the next row to it.
const RESOLVED: LinkReading = LinkReading {
    raw_url_wraps: false,
    windows_paths: false,
};

type Rows = Vec<(u16, Range<u16>)>;

/// What `frame` reads at a cell, over the whole frame.
fn read(frame: &FrameData, column: u16, row: u16) -> Option<(RowTarget, Rows)> {
    frame_link(frame, column, row, 0, frame.width).map(|link| (link.target, link.rows))
}

fn read_resolved(frame: &FrameData, column: u16, row: u16) -> Option<(RowTarget, Rows)> {
    let bounds = Bounds {
        columns: 0..frame.width,
        rows: 0..frame.height,
    };
    RESOLVED
        .frame_link(frame, column, row, &bounds)
        .map(|link| (link.target, link.rows))
}

fn web(url: &str, rows: &[(u16, Range<u16>)]) -> Option<(RowTarget, Rows)> {
    Some((RowTarget::Web(url.into()), rows.to_vec()))
}

fn file(path: &str, line: Option<u32>, rows: &[(u16, Range<u16>)]) -> Option<(RowTarget, Rows)> {
    let target = RowTarget::Path {
        path: path.into(),
        line,
        query: None,
        explicit: false,
        unjoined: None,
    };
    Some((target, rows.to_vec()))
}

/// A path a raw wrap joined, with the path its first row reads alone.
fn joined_file(
    path: &str,
    line: Option<u32>,
    first: &str,
    rows: &[(u16, Range<u16>)],
) -> Option<(RowTarget, Rows)> {
    let mut found = file(path, line, rows);
    if let Some((RowTarget::Path { unjoined, .. }, _)) = &mut found {
        *unjoined = Some(first.into());
    }
    found
}

/// The path the first row of the path read at a cell reads alone.
fn unjoined(frame: &FrameData, column: u16, row: u16) -> Option<String> {
    match read(frame, column, row) {
        Some((RowTarget::Path { unjoined, .. }, _)) => unjoined,
        other => panic!("no path: {other:?}"),
    }
}

#[test]
fn a_raw_wrap_joins_an_address_read_from_either_row() {
    let wrapped = rows(&["see https://exa.com/", "abc/def next"], 20);
    let expected = web("https://exa.com/abc/def", &[(0, 4..20), (1, 0..7)]);
    assert_eq!(read(&wrapped, 5, 0), expected);
    assert_eq!(read(&wrapped, 2, 1), expected);
}

#[test]
fn a_raw_wrap_joins_a_path_and_its_location() {
    let wrapped = rows(&["build src/very/long/", "file.rs:12 ok"], 20);
    let expected = joined_file(
        "src/very/long/file.rs",
        Some(12),
        "src/very/long/",
        &[(0, 6..20), (1, 0..10)],
    );
    assert_eq!(read(&wrapped, 8, 0), expected);
    // A daemon that resolves links reads no paths, so they join anyway.
    assert_eq!(read_resolved(&wrapped, 8, 0), expected);
}

#[test]
fn a_row_starting_with_a_blank_does_not_continue_the_one_above() {
    let separate = rows(&["see https://exa.com/", " abc/def next"], 20);
    assert_eq!(
        read(&separate, 5, 0),
        web("https://exa.com/", &[(0, 4..20)])
    );
}

#[test]
fn a_link_at_the_edge_over_a_blank_row_is_complete_unless_it_ends_the_pane() {
    for (text, expected) in [
        (
            "see https://exa.com/",
            web("https://exa.com/", &[(0, 4..20)]),
        ),
        (
            "build src/very/long/",
            file("src/very/long/", None, &[(0, 6..20)]),
        ),
    ] {
        assert_eq!(read(&rows(&[text, ""], 20), 8, 0), expected, "{text}");
        // On the pane's bottom row it may continue off-screen.
        assert_eq!(read(&rows(&[text], 20), 8, 0), None, "{text}");
    }
}

#[test]
fn a_wide_glyph_that_did_not_fit_continues_on_the_next_row() {
    let mut wrapped = rows(&["ab src/aaaaaaaaaaaa", "界 .rs ok"], 20);
    wrapped.cells[21].skip = true;
    assert_eq!(
        read(&wrapped, 5, 0),
        joined_file(
            "src/aaaaaaaaaaaa界.rs",
            None,
            "src/aaaaaaaaaaaa",
            &[(0, 3..19), (1, 0..5)]
        )
    );
}

#[test]
fn a_path_a_raw_wrap_joined_keeps_what_its_first_row_reads() {
    // `abc.txt` may have ended at the pane's edge, over a line of its own.
    let wrapped = rows(&["/srv/project/abc.txt", "other.txt ok"], 20);
    assert_eq!(
        read(&wrapped, 3, 0),
        joined_file(
            "/srv/project/abc.txtother.txt",
            None,
            "/srv/project/abc.txt",
            &[(0, 0..20), (1, 0..9)]
        )
    );
    assert_eq!(
        unjoined(&wrapped, 3, 1),
        Some("/srv/project/abc.txt".into())
    );
}

#[test]
fn a_path_joined_only_inside_its_location_reads_the_same_on_its_first_row() {
    let wrapped = rows(&["see /srv/proj/a.rs:1", "2 ok"], 20);
    assert_eq!(
        read(&wrapped, 6, 0),
        file("/srv/proj/a.rs", Some(12), &[(0, 4..20), (1, 0..1)])
    );
}

#[test]
fn a_path_an_application_wrapped_or_not_joined_has_no_first_row_reading() {
    let result = rows(&["  ⎿  /home/u/pj/src/", "     deep/file.rs"], 20);
    assert_eq!(unjoined(&result, 7, 0), None);
    let apart = rows(&["see /srv/a.txt", "other.txt ok"], 20);
    assert_eq!(unjoined(&apart, 6, 0), None);
}

#[test]
fn a_raw_wrapped_address_is_left_to_a_daemon_that_resolves_links() {
    let wrapped = rows(&["see https://exa.com/", "abc/def next"], 20);
    assert_eq!(read_resolved(&wrapped, 5, 0), None);
    assert_eq!(read_resolved(&wrapped, 2, 1), None);
}

#[test]
fn an_address_wrapping_past_the_row_cap_is_no_link() {
    let mut lines = vec!["https://e.co/aaaaaaa".to_owned()];
    lines.extend((0..199).map(|_| "a".repeat(20)));
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let long = rows(&lines, 20);
    assert_eq!(read(&long, 2, 0), None);
    assert_eq!(read(&long, 2, 100), None);
}

#[test]
fn a_popup_and_a_pane_read_only_their_own_rows() {
    let mut s = surface("");
    let popup = |lines: &[&str]| {
        Some(Box::new(ClientShellPopupSurface {
            terminal_id: "popup".into(),
            title: String::new(),
            width: None,
            height: None,
            frame: rows(lines, 20),
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            pixel_width: 200,
            pixel_height: 40,
        }))
    };
    s.popup = popup(&["see https://exa.com/", "abc/def next"]);
    assert_eq!(
        link_at(&s, 351., 21., 10., 20.).as_deref(),
        Some("https://exa.com/abc/def")
    );
    assert_eq!(
        link_at(&s, 311., 41., 10., 20.).as_deref(),
        Some("https://exa.com/abc/def")
    );
    // The popup's last row is its bottom, whatever lies below it.
    s.popup = popup(&["xx", "see https://exa.com/"]);
    assert!(link_at(&s, 351., 41., 10., 20.).is_none());

    // A row above the pane, such as a tab bar, is never its text.
    let mut s = surface(&format!("{}{}src/a.rs ok", "a".repeat(20), " ".repeat(60)));
    s.panes[0].inner_rect = SurfaceRect {
        x: 0,
        y: 1,
        width: 20,
        height: 3,
    };
    let link = pane_link_at(&s, 21., 21., 10., 20.).unwrap();
    assert_eq!(
        Some((link.link.target, link.link.rows)),
        file("src/a.rs", None, &[(1, 0..8)])
    );
}

#[test]
fn a_word_wrapped_reply_continues_under_the_text_after_its_marker() {
    let reply = rows(&["● see https://ex.co/", "  aaaaaaaa/b.html ok"], 20);
    let expected = web("https://ex.co/aaaaaaaa/b.html", &[(0, 6..20), (1, 2..17)]);
    assert_eq!(read(&reply, 8, 0), expected);
    assert_eq!(read(&reply, 4, 1), expected);
}

#[test]
fn a_row_indented_past_the_text_is_a_new_line_and_the_link_is_complete() {
    let indented = rows(&["● see https://ex.co/", "   more text"], 20);
    assert_eq!(read(&indented, 8, 0), web("https://ex.co/", &[(0, 6..20)]));
}

#[test]
fn a_word_that_fits_the_next_row_whole_was_not_broken_there() {
    let moved = rows(&["● see https://ex.co/", "  more text"], 20);
    assert_eq!(read(&moved, 8, 0), web("https://ex.co/", &[(0, 6..20)]));
}

#[test]
fn a_word_wrap_inside_a_box_joins_between_its_walls() {
    let boxed = rows(
        &[
            "╭──────────────────╮",
            "│ see https://ex.c │",
            "│ o/aaaaaaaaa/b ok │",
            "╰──────────────────╯",
        ],
        20,
    );
    let expected = web("https://ex.co/aaaaaaaaa/b", &[(1, 6..18), (2, 2..15)]);
    assert_eq!(read(&boxed, 8, 1), expected);
    assert_eq!(read(&boxed, 3, 2), expected);
}

#[test]
fn a_tool_result_continues_under_the_text_after_its_marker() {
    let result = rows(&["  ⎿  /home/u/pj/src/", "     deep/file.rs"], 20);
    assert_eq!(
        read(&result, 7, 0),
        file(
            "/home/u/pj/src/deep/file.rs",
            None,
            &[(0, 5..20), (1, 5..17)]
        )
    );
}

#[test]
fn a_word_ending_before_the_blocks_wrap_column_does_not_join() {
    let block = rows(
        &[
            "  words that go far ok",
            "  see https://e.co/a",
            "  bcd/e next",
        ],
        22,
    );
    assert_eq!(read(&block, 8, 1), web("https://e.co/a", &[(1, 6..20)]));
}

#[test]
fn a_table_cell_joins_only_when_the_rest_of_the_next_record_is_blank() {
    let record = rows(&["│ https://a.co/x │ y │", "│ next           │ z │"], 22);
    assert_eq!(read(&record, 4, 0), web("https://a.co/x", &[(0, 2..16)]));
    let wrapped = rows(&["│ https://a.co/x │ y │", "│ ext/b          │   │"], 22);
    assert_eq!(
        read(&wrapped, 4, 0),
        web("https://a.co/xext/b", &[(0, 2..16), (1, 2..7)])
    );
}

#[test]
fn side_by_side_panels_wrap_each_at_its_own_walls() {
    let panels = rows(
        &[
            "│ src/ab │ │ see https://e.c │",
            "│ cd.rs  │ │ o/xxxxxx ok     │",
        ],
        30,
    );
    assert_eq!(
        read(&panels, 3, 0),
        file("src/abcd.rs", None, &[(0, 2..8), (1, 2..7)])
    );
    assert_eq!(
        read(&panels, 20, 0),
        web("https://e.co/xxxxxx", &[(0, 17..28), (1, 13..21)])
    );
}

#[test]
fn parentheses_balance_across_a_wrap() {
    let wrapped = rows(&["see https://e.co/F_(", "bar) ok"], 20);
    assert_eq!(
        read(&wrapped, 6, 0),
        web("https://e.co/F_(bar)", &[(0, 4..20), (1, 0..4)])
    );
}

#[test]
fn a_numbered_row_continuing_under_its_text_joins_as_after_a_marker() {
    let numbered = rows(&["  12 foo https://exa", "     mple.com/x"], 20);
    assert_eq!(
        read(&numbered, 10, 0),
        web("https://example.com/x", &[(0, 9..20), (1, 5..15)])
    );
}

#[test]
fn text_cut_off_with_an_ellipsis_is_no_link() {
    let cut = rows(&["see https://e.co/ab…", "", "see src/a/very/lo… ok"], 21);
    assert_eq!(read(&cut, 6, 0), None);
    assert_eq!(read(&cut, 6, 2), None);
}

#[test]
fn an_explicit_hyperlink_continues_on_the_next_row() {
    let mut linked = rows(&["see: aaaaaaaaaaaaaaa", "bbbbb after", ""], 20);
    linked.hyperlinks = vec!["https://example.com/doc".into()];
    // The last 15 cells of the first row and the first 5 of the second.
    for index in 5..25 {
        linked.cells[index].hyperlink = Some(0);
    }
    let expected = web("https://example.com/doc", &[(0, 5..20), (1, 0..5)]);
    assert_eq!(read(&linked, 6, 0), expected);
    assert_eq!(read(&linked, 2, 1), expected);
}

#[test]
fn a_link_strictly_contains_only_what_it_covers_and_more() {
    let local = |rows: &[(u16, Range<u16>)]| RowLink {
        target: RowTarget::Web("https://ex.co/".into()),
        rows: rows.to_vec(),
    };
    let wrapped = local(&[(0, 6..20), (1, 2..17)]);
    // The same cells are not more of them.
    assert!(!wrapped.strictly_contains(&[(0, 6..20), (1, 2..17)]));
    // Another row, or more of the same row, is.
    assert!(wrapped.strictly_contains(&[(0, 6..20)]));
    assert!(local(&[(0, 2..20)]).strictly_contains(&[(0, 6..20)]));
    // Cells on a row the link does not reach, or past its columns, are not
    // covered.
    assert!(!local(&[(0, 6..20)]).strictly_contains(&[(1, 6..10)]));
    assert!(!wrapped.strictly_contains(&[(0, 4..12)]));
}

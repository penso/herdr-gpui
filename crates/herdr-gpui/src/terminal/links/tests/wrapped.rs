use super::*;
use wrap::MAX_WRAPPED_ROWS;

/// What the detector reads at a cell of a frame that is one whole area.
fn read(frame: &FrameData, column: u16, row: u16) -> Option<RowLink> {
    frame_link(frame, column, row, 0..frame.width, 0..frame.height)
}

/// A frame whose rows each hold one line, as a program that ends its lines
/// with a newline prints them, padded with blanks to `width`.
fn lines(rows: &[&str], width: u16) -> FrameData {
    let mut frame = frame("", width, rows.len() as u16);
    for (row, text) in rows.iter().enumerate() {
        let start = row * usize::from(width);
        for (cell, symbol) in frame.cells[start..].iter_mut().zip(text.chars()) {
            cell.symbol = symbol.to_string();
        }
    }
    frame
}

fn bare(path: &str, line: Option<u32>) -> RowTarget {
    RowTarget::Path {
        path: path.into(),
        line,
        explicit: false,
    }
}

/// Every cell of the link at `cells` reads the same link.
fn read_everywhere(frame: &FrameData, cells: &[(u16, u16)]) -> Option<RowLink> {
    let link = read(frame, cells[0].0, cells[0].1);
    for &(column, row) in cells {
        assert_eq!(read(frame, column, row), link, "({column}, {row})");
    }
    link
}

#[test]
fn a_path_a_terminal_wrapped_reads_whole_from_either_row() {
    // `see /tmp/project/src/main.rs:12 ok` in 20 columns.
    let wrapped = frame("see /tmp/project/src/main.rs:12 ok", 20, 3);
    let link = read_everywhere(&wrapped, &[(4, 0), (19, 0), (0, 1), (10, 1)]).unwrap();
    assert_eq!(link.target, bare("/tmp/project/src/main.rs", Some(12)));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 4..20), (1, 0..11)]
    );
    // The words around it are not part of it.
    assert_ne!(
        read(&wrapped, 1, 0).map(|link| link.target),
        Some(link.target.clone())
    );
    assert_ne!(
        read(&wrapped, 13, 1).map(|link| link.target),
        Some(link.target)
    );
}

#[test]
fn a_path_wrapped_across_three_rows_reads_whole_from_the_middle_one() {
    // The middle row is all path, and touches both edges.
    let wrapped = frame("cat /usr/local/share/doc/readme.md", 12, 4);
    let link = read_everywhere(&wrapped, &[(5, 1), (4, 0), (0, 1), (11, 1), (9, 2)]).unwrap();
    assert_eq!(link.target, bare("/usr/local/share/doc/readme.md", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 4..12), (1, 0..12), (2, 0..10)]
    );
}

// Windows file URLs name drive paths, which these Unix fixtures do not.
#[cfg(unix)]
#[test]
fn a_wrapped_file_address_names_its_file() {
    let wrapped = frame("open file:///tmp/some%20dir/a.txt now", 16, 3);
    let link = read_everywhere(&wrapped, &[(5, 0), (8, 1), (0, 2)]).unwrap();
    assert_eq!(link.target, bare("/tmp/some dir/a.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 5..16), (1, 0..16), (2, 0..1)]
    );
    // Split inside its scheme, the next row alone names nothing: the row
    // under the pointer is read with the one it continues.
    let split = frame("see it: file:///tmp/a.txt", 10, 3);
    let link = read_everywhere(&split, &[(2, 1), (8, 0), (4, 2)]).unwrap();
    assert_eq!(link.target, bare("/tmp/a.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 8..10), (1, 0..10), (2, 0..5)]
    );
}

#[test]
fn addresses_across_rows_are_not_guessed() {
    // A web address across rows is the daemon's to read, from the terminal's
    // wrap flags; the surface cannot tell a wrap from a full line.
    let web = frame("go https://example.com/a/b/c now", 16, 3);
    for (column, row) in [(3, 0), (15, 0), (0, 1), (2, 1)] {
        assert!(read(&web, column, row).is_none(), "({column}, {row})");
    }
    // Nothing confirms another scheme's address either.
    let mail = frame("mail mailto:someone@example.com", 16, 2);
    for (column, row) in [(5, 0), (3, 1)] {
        assert!(read(&mail, column, row).is_none(), "({column}, {row})");
    }
    // Each still reads on a row of its own.
    let web = frame("go https://example.com/a now", 40, 1);
    assert_eq!(
        read(&web, 3, 0).map(|link| link.target),
        Some(RowTarget::Web("https://example.com/a".into()))
    );
}

#[test]
fn lines_that_end_short_of_the_right_edge_are_never_joined() {
    let separate = lines(&["see /tmp/a", "b/c.txt here"], 20);
    let first = read(&separate, 5, 0).unwrap();
    assert_eq!(first.target, bare("/tmp/a", None));
    assert_eq!(first.rows().collect::<Vec<_>>(), vec![(0, 4..10)]);
    let second = read(&separate, 2, 1).unwrap();
    assert_eq!(second.target, bare("b/c.txt", None));
    assert_eq!(second.rows().collect::<Vec<_>>(), vec![(1, 0..7)]);
}

#[test]
fn a_link_ending_at_the_right_edge_keeps_to_its_row() {
    // The next line is empty, so nothing continues the path; it may still go
    // on past the edge, clipped as `less -S` clips a line, so it is not read,
    // as before.
    let ended = lines(&["error in src/lib.rs", "", "  help: src/a.rs"], 19);
    assert!(read(&ended, 12, 0).is_none());
    let below = read(&ended, 10, 2).unwrap();
    assert_eq!(below.target, bare("src/a.rs", None));
    assert_eq!(below.rows().collect::<Vec<_>>(), vec![(2, 8..16)]);
    // A web address filling its row, then a line that starts with text,
    // looks like one wrapped address: neither row reads one.
    let url = lines(&["https://example.com/x", "Done"], 21);
    assert!(read(&url, 3, 0).is_none());
    assert!(read(&url, 1, 1).is_none());
}

#[test]
fn a_path_an_agent_wrapped_under_a_hanging_indent_reads_whole() {
    // An agent's markdown list, wrapped by the agent itself: the path breaks
    // at the right edge and goes on under the item's text, after an indent.
    let item = "  - round 1: /Users/dominik/Projects/out/shots-before/tile";
    let width = item.chars().count() as u16;
    let listed = lines(
        &[item, "    77-street2-round1.png", "  - round 2: done"],
        width,
    );
    let link = read_everywhere(&listed, &[(13, 0), (width - 1, 0), (4, 1), (23, 1)]).unwrap();
    assert_eq!(
        link.target,
        bare(
            "/Users/dominik/Projects/out/shots-before/tile77-street2-round1.png",
            None
        )
    );
    // The indent is no part of it.
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 13..width), (1, 4..25)]
    );
    // Nor does the next item continue it.
    assert!(read(&listed, 1, 1).is_none());
    assert_ne!(
        read(&listed, 14, 2).map(|link| link.target),
        Some(link.target)
    );
}

#[test]
fn an_indented_line_goes_on_only_from_a_path() {
    // Prose that happens to fill its row: the indented path below it is a
    // line of its own, read alone as before.
    let prose = lines(&["the build is green", "    src/main.rs ok"], 18);
    let link = read(&prose, 6, 1).unwrap();
    assert_eq!(link.target, bare("src/main.rs", None));
    assert_eq!(link.rows().collect::<Vec<_>>(), vec![(1, 4..15)]);
    // A web address that seems to go on after an indent is still not
    // guessed.
    let web = lines(&["see https://example.com/a", "    /b now"], 25);
    assert!(read(&web, 5, 0).is_none());
    assert!(read(&web, 4, 1).is_none());
}

#[test]
fn a_long_name_an_agent_wrapped_over_several_rows_reads_whole() {
    // Only the first row names a folder; the rows below hold the rest of one
    // file name, so the separator that lets an indented row go on is rows
    // above the second break.
    let listed = lines(
        &["see /tmp/abcdefg", "    hijklmnopqrs", "    tuvw.txt ok"],
        16,
    );
    let link = read_everywhere(
        &listed,
        &[(4, 0), (15, 0), (4, 1), (15, 1), (4, 2), (11, 2)],
    )
    .unwrap();
    assert_eq!(link.target, bare("/tmp/abcdefghijklmnopqrstuvw.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 4..16), (1, 4..16), (2, 4..12)]
    );
    // The same name with its middle row wrapped by the terminal.
    let mixed = lines(
        &["see /tmp/abcdefg", "hijklmnopqrstuvw", "    x.txt ok"],
        16,
    );
    let link = read_everywhere(&mixed, &[(5, 0), (8, 1), (5, 2)]).unwrap();
    assert_eq!(link.target, bare("/tmp/abcdefghijklmnopqrstuvwx.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 4..16), (1, 0..16), (2, 4..9)]
    );
    // Prose filling its rows still leaves the indented path below it alone,
    // however many rows its last word spans.
    let prose = lines(
        &["the build is all", "    greenfieldsx", "    src/main.rs"],
        16,
    );
    let link = read(&prose, 6, 2).unwrap();
    assert_eq!(link.target, bare("src/main.rs", None));
    assert_eq!(link.rows().collect::<Vec<_>>(), vec![(2, 4..15)]);
}

#[test]
fn an_escaped_space_stays_part_of_a_wrapped_path() {
    let text = r"see /tmp/a\ b/verylongname.txt ok";
    let path = bare("/tmp/a b/verylongname.txt", None);
    // Wrapped after `b/ve`: the escaped space does not stop the part before
    // it from reading down, so every part names the same file.
    let after_name = frame(text, 16, 3);
    let link = read_everywhere(
        &after_name,
        &[(4, 0), (9, 0), (11, 0), (12, 0), (0, 1), (13, 1)],
    )
    .unwrap();
    assert_eq!(link.target, path);
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 4..16), (1, 0..14)]
    );
    // Wrapped right after the backslash: the space starting the next row is
    // the escaped one, not an indent, and stays in the name.
    let after_backslash = frame(text, 11, 4);
    let link =
        read_everywhere(&after_backslash, &[(4, 0), (10, 0), (0, 1), (1, 1), (3, 2)]).unwrap();
    assert_eq!(link.target, path);
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 4..11), (1, 0..11), (2, 0..8)]
    );
    // A drive's backslash escapes nothing, so the blanks below one ending a
    // row are an indent, which an agent's wrapped path goes on after.
    let drive = lines(&[r"open C:\dir\sub\", "    file.txt ok"], 16);
    let link = read_everywhere(&drive, &[(5, 0), (15, 0), (6, 1)]).unwrap();
    assert_eq!(link.target, bare(r"C:\dir\sub\file.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 5..16), (1, 4..12)]
    );
}

#[test]
fn a_drive_path_an_agent_wrapped_over_three_rows_reads_whole() {
    // The middle row's indent is left out of the line, so the drive its
    // path started with still keeps the backslash ending that row from
    // escaping the indent below it.
    let frame = lines(
        &[r"open C:\dir\sub\", r"    folder\more\", "    file.txt ok"],
        16,
    );
    let link =
        read_everywhere(&frame, &[(5, 0), (15, 0), (4, 1), (15, 1), (4, 2), (11, 2)]).unwrap();
    assert_eq!(link.target, bare(r"C:\dir\sub\folder\more\file.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(0, 5..16), (1, 4..16), (2, 4..12)]
    );
}

#[test]
fn a_drive_path_under_a_full_prose_row_reads_whole() {
    // The prose does not go on into the indented path, so the path's row is
    // read afresh and its drive keeps the backslash ending it from escaping
    // the indent below.
    let frame = lines(
        &["all tests passed", r"    C:\dir\more\", "    file.txt ok"],
        16,
    );
    let link = read_everywhere(&frame, &[(4, 1), (15, 1), (4, 2), (11, 2)]).unwrap();
    assert_eq!(link.target, bare(r"C:\dir\more\file.txt", None));
    assert_eq!(
        link.rows().collect::<Vec<_>>(),
        vec![(1, 4..16), (2, 4..12)]
    );
}

#[test]
fn wide_characters_in_a_wrapped_path_keep_their_columns() {
    let cell = |symbol: &str| CellData {
        symbol: symbol.into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    };
    let continuation = CellData {
        skip: true,
        ..cell("")
    };
    // `ls /tmp/资料/a.txt`, the first wide glyph in the last two columns.
    let mut wide = frame("", 10, 2);
    let first = "ls /tmp/".chars().map(|c| cell(&c.to_string()));
    let second = "/a.txt".chars().map(|c| cell(&c.to_string()));
    let row: Vec<_> = first
        .chain([cell("资"), continuation.clone()])
        .chain([cell("料"), continuation])
        .chain(second)
        .collect();
    wide.cells[..row.len()].clone_from_slice(&row);
    let link = read_everywhere(&wide, &[(3, 0), (8, 0), (9, 0), (0, 1), (1, 1), (7, 1)]).unwrap();
    assert_eq!(link.target, bare("/tmp/资料/a.txt", None));
    assert_eq!(link.rows().collect::<Vec<_>>(), vec![(0, 3..10), (1, 0..8)]);
}

#[test]
fn a_link_is_read_across_a_bounded_number_of_rows() {
    let width = 8;
    let rows = MAX_WRAPPED_ROWS + 2;
    let fits = format!(
        "/{} end",
        "a".repeat(usize::from(width * (MAX_WRAPPED_ROWS - 1)))
    );
    let fitting = frame(&fits, width, rows);
    let last = MAX_WRAPPED_ROWS - 1;
    let link = read_everywhere(&fitting, &[(0, 0), (3, last / 2), (0, last)]).unwrap();
    assert_eq!(link.rows().count(), usize::from(MAX_WRAPPED_ROWS));
    // One row longer, it is not read from anywhere along it.
    let longer = format!("/{} end", "a".repeat(usize::from(width * MAX_WRAPPED_ROWS)));
    let long = frame(&longer, width, rows);
    for row in [0, MAX_WRAPPED_ROWS / 2, MAX_WRAPPED_ROWS] {
        assert!(read(&long, 1, row).is_none(), "{row}");
    }
}

#[test]
fn a_wrapped_link_stays_inside_its_pane() {
    // The pane is the frame's 20 left columns; to its right is another
    // pane's text, which the path's first row does not run into.
    let mut s = surface("");
    s.frame = lines(&["see /tmp/project/srczzzz", "/main.rs ok"], 80);
    s.frame.cells.extend(frame("", 80, 2).cells);
    s.frame.height = 4;
    s.panes[0].inner_rect.width = 20;
    let link = pane_link_at(&s, 11., 21., 10., 20.).unwrap().link;
    assert_eq!(link.target, bare("/tmp/project/src/main.rs", None));
    assert_eq!(link.rows().collect::<Vec<_>>(), vec![(0, 4..20), (1, 0..8)]);
    // Nor does it reach into the row above the pane, which is not its text.
    s.panes[0].inner_rect.y = 1;
    s.panes[0].inner_rect.height = 3;
    let link = pane_link_at(&s, 11., 21., 10., 20.).unwrap().link;
    assert_eq!(link.target, bare("/main.rs", None));
    assert_eq!(link.rows().collect::<Vec<_>>(), vec![(1, 0..8)]);
    // A path running into the pane's last row may go on below it.
    s.panes[0].inner_rect.y = 0;
    s.panes[0].inner_rect.height = 1;
    assert!(pane_link_at(&s, 61., 1., 10., 20.).is_none());
}

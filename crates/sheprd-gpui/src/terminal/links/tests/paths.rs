use super::*;

/// What the detector reads in a one-row frame at `column`.
fn read(text: &str, column: u16) -> Option<RowLink> {
    frame_link(&frame(text, 60, 1), column, 0, 0..60, 0..1)
}

fn path(text: &str, column: u16) -> Option<(String, Range<u16>)> {
    located(text, column).map(|(path, _, columns)| (path, columns))
}

fn located(text: &str, column: u16) -> Option<(String, Option<u32>, Range<u16>)> {
    read(text, column).and_then(|link| match link.target {
        RowTarget::Path { path, line, .. } => Some((path, line, link.columns)),
        RowTarget::Web(_)
        | RowTarget::External(_)
        | RowTarget::OtherHost(_)
        | RowTarget::Malformed => None,
    })
}

fn bare(path: &str) -> RowTarget {
    RowTarget::Path {
        path: path.into(),
        line: None,
        explicit: false,
    }
}

#[test]
fn a_location_suffix_names_the_line() {
    for (text, column, line) in [
        ("error: src/main.rs:12:5: oops", 9, Some(12)),
        ("--> main.rs:3", 5, Some(3)),
        ("modified:   crates/a/b.rs", 20, None),
    ] {
        assert_eq!(
            located(text, column).map(|found| found.1),
            Some(line),
            "{text}"
        );
    }
}

#[test]
fn printed_paths_read_whole_with_their_location_underlined() {
    for (text, column, expected, columns) in [
        ("error: src/main.rs:12:5: oops", 9, "src/main.rs", 7..23),
        ("File \"/abs/file.py\", line 3", 8, "/abs/file.py", 6..18),
        ("see ~/notes/todo.md. next", 6, "~/notes/todo.md", 4..19),
        ("(./build/out) ok", 3, "./build/out", 1..12),
        ("--> main.rs:3", 5, "main.rs", 4..13),
        ("modified:   crates/a/b.rs", 20, "crates/a/b.rs", 12..25),
        ("ls src/ now", 4, "src/", 3..7),
    ] {
        assert_eq!(
            path(text, column),
            Some((expected.into(), columns)),
            "{text}"
        );
    }
}

#[test]
fn any_word_may_name_a_file_in_the_pane_directory() {
    for (text, column, expected, columns) in [
        ("README.md is here", 2, "README.md", 0..9),
        ("see Makefile now", 5, "Makefile", 4..12),
        ("ls src now", 4, "src", 3..6),
        ("version v1.2 out", 9, "v1.2", 8..12),
        // Punctuation closing a sentence is not part of the name.
        ("a src/a.rs. b", 3, "src/a.rs", 2..10),
    ] {
        assert_eq!(
            path(text, column),
            Some((expected.into(), columns)),
            "{text}"
        );
    }
}

#[test]
fn text_that_cannot_name_a_local_file_is_not_read_as_one() {
    for (text, column) in [
        ("cd ~user/x", 5),
        ("cd ../ now", 4),
        ("scp host:/path now", 6),
        ("a src/a.rs. b", 10),
        ("a src/a.rs b", 1),
        ("ssh://example.com", 3),
    ] {
        assert!(read(text, column).is_none(), "{text}");
    }
    // A URL stays a web link, never a path.
    assert_eq!(
        read("go https://example.com/a/b now", 5).map(|link| link.target),
        Some(RowTarget::Web("https://example.com/a/b".into()))
    );
}

#[test]
fn paths_are_pane_links_only_and_never_web_links() {
    let s = surface("open src/lib.rs:4 now");
    assert!(link_at(&s, 61., 1., 10., 20.).is_none());
    let link = pane_link_at(&s, 61., 1., 10., 20.).unwrap();
    assert_eq!(link.pane_id, "pane");
    assert_eq!(
        link.link.target,
        RowTarget::Path {
            path: "src/lib.rs".into(),
            line: Some(4),
            explicit: false,
        }
    );
    assert_eq!((link.link.row, link.link.columns), (0, 5..17));
}

#[test]
fn paths_that_may_wrap_are_not_guessed() {
    // Running to the right edge, the path may continue on the next row.
    assert!(frame_link(&frame("ab src/long/path", 16, 1), 5, 0, 0..16, 0..1).is_none());
    // Starting a row under a full one, it may be the tail of that row.
    let wrapped = frame("https://example.com/src/a.rs", 20, 2);
    assert!(frame_link(&wrapped, 2, 1, 0..20, 0..2).is_none());
    let mut separate = frame("first line ends here", 20, 2);
    separate.cells[19].symbol = " ".into();
    for (cell, symbol) in separate.cells[20..].iter_mut().zip("src/a.rs".chars()) {
        cell.symbol = symbol.to_string();
    }
    assert_eq!(
        frame_link(&separate, 2, 1, 0..20, 0..2).map(|link| link.target),
        Some(bare("src/a.rs"))
    );
}

#[test]
fn wide_characters_before_a_path_keep_its_columns() {
    let mut wide = frame("界 src/a.rs", 20, 1);
    wide.cells.insert(
        1,
        CellData {
            skip: true,
            ..wide.cells[0].clone()
        },
    );
    wide.cells.pop();
    assert_eq!(
        frame_link(&wide, 4, 0, 0..20, 0..1).map(|link| link.columns),
        Some(3..11)
    );
}

// Windows file URLs name drive paths, which these Unix fixtures do not.
#[cfg(unix)]
#[test]
fn a_hyperlink_names_a_path_by_a_file_address_or_without_a_scheme() {
    let named = |path: &str| RowTarget::Path {
        path: path.into(),
        line: None,
        explicit: true,
    };
    let mut s = surface("report");
    s.frame.cells[0].hyperlink = Some(0);
    for (destination, expected) in [
        ("file:///tmp/a%20b", Some(named("/tmp/a b"))),
        ("file://localhost/tmp/a", Some(named("/tmp/a"))),
        // Whether the file is there is the click's to find out.
        ("file:///no/such/file", Some(named("/no/such/file"))),
        // A file on another host is a link that never opens here.
        (
            "file://elsewhere/tmp/a",
            Some(RowTarget::OtherHost("elsewhere".into())),
        ),
        // Without a scheme the target is a path as a program prints one,
        // left for the click to resolve. A location is not read from it.
        ("/tmp/a", Some(named("/tmp/a"))),
        ("~/notes.md", Some(named("~/notes.md"))),
        ("./src/a.rs", Some(named("./src/a.rs"))),
        ("../src/a.rs", Some(named("../src/a.rs"))),
        ("src/a.rs:12", Some(named("src/a.rs:12"))),
        // A target that is no address or path is still an explicit link.
        ("", Some(RowTarget::Malformed)),
        ("src/\u{7}a.rs", Some(RowTarget::Malformed)),
        ("https://", Some(RowTarget::Malformed)),
    ] {
        s.frame.hyperlinks = vec![destination.into()];
        assert_eq!(
            pane_link_at(&s, 1., 1., 10., 20.).map(|link| link.link.target),
            expected,
            "{destination}"
        );
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
    }
    // A target longer than a path can be is not one.
    s.frame.hyperlinks = vec![format!("/{}", "a".repeat(4096))];
    assert_eq!(
        pane_link_at(&s, 1., 1., 10., 20.).map(|link| link.link.target),
        Some(RowTarget::Malformed)
    );
}

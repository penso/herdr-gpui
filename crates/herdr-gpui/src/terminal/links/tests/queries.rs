//! Paths that end in a page's `?query` or `#fragment`, as a mock page's
//! address does: the path part is what names the file, and the link covers
//! the whole word.
use super::*;

/// The path, line, query, and columns a one-row frame reads at `column`.
type Read = (String, Option<u32>, Option<String>, Range<u16>);

fn read(text: &str, column: u16) -> Option<Read> {
    let link = frame_link(&frame(text, 60, 1), column, 0, 0, 60)?;
    let columns = columns(&link);
    match link.target {
        RowTarget::Path {
            path, line, query, ..
        } => Some((path, line, query, columns)),
        RowTarget::Web(_)
        | RowTarget::External(_)
        | RowTarget::OtherHost(_)
        | RowTarget::Malformed => None,
    }
}

fn split(path: &str, line: Option<u32>, query: Option<&str>, columns: Range<u16>) -> Option<Read> {
    Some((path.into(), line, query.map(Into::into), columns))
}

#[test]
fn a_page_s_query_or_fragment_is_split_from_its_path_and_underlined_with_it() {
    for (text, column, expected) in [
        (
            r"open X:\a\index.html?sheet=1 now",
            6,
            split(r"X:\a\index.html", None, Some("?sheet=1"), 5..28),
        ),
        (
            "/srv/query?s=25 ok",
            1,
            split("/srv/query", None, Some("?s=25"), 0..15),
        ),
        (
            "src/main.rs#L10 ok",
            1,
            split("src/main.rs", Some(10), Some("#L10"), 0..15),
        ),
        (
            "src/main.rs#L10-L20 ok",
            1,
            split("src/main.rs", Some(10), Some("#L10-L20"), 0..19),
        ),
        // A printed location wins over the fragment's line.
        (
            "src/main.rs:7#L10 ok",
            1,
            split("src/main.rs", Some(7), Some("#L10"), 0..17),
        ),
        // A colon in the query is no scheme's, host's, or drive's.
        (
            r"X:\a\b.html?t=12:30 ok",
            1,
            split(r"X:\a\b.html", None, Some("?t=12:30"), 0..19),
        ),
        (
            "/a/b.html?x=1#top ok",
            1,
            split("/a/b.html", None, Some("?x=1#top"), 0..17),
        ),
        (
            "see docs/intro.md#usage ok",
            5,
            split("docs/intro.md", None, Some("#usage"), 4..23),
        ),
        // A PDF's page is a fragment too.
        (
            "see docs/guide.PDF#page=3 ok",
            5,
            split("docs/guide.PDF", None, Some("#page=3"), 4..25),
        ),
    ] {
        assert_eq!(read(text, column), expected, "{text}");
    }
}

#[test]
fn a_question_mark_closing_a_sentence_is_no_query() {
    assert_eq!(
        read("see index.html?. ok", 5),
        split("index.html", None, None, 4..14)
    );
}

#[test]
fn marks_that_start_no_page_s_suffix_keep_the_path_whole() {
    for (text, expected) in [
        (r"C:\repo\.tmp\#notes!.md", Some(r"C:\repo\.tmp\#notes!.md")),
        (r"/tmp/a\?b", Some("/tmp/a?b")),
        ("/tmp/a?b", Some("/tmp/a?b")),
        ("~/.emacs.d/#init.el#", Some("~/.emacs.d/#init.el#")),
        ("notes#1", Some("notes#1")),
        ("src/a.rs#", Some("src/a.rs#")),
        ("src/a.rs#1x", Some("src/a.rs#1x")),
        // Only a PDF has pages.
        ("docs/a.md#page=3", Some("docs/a.md#page=3")),
        ("docs/a.pdf#page=", Some("docs/a.pdf#page=")),
        // Rust's verbatim prefix is never split; its drive's colon then
        // makes it no POSIX path at all.
        (r"\\?\C:\x\a.rs", None),
    ] {
        let text = format!("{text} ok");
        let found = read(&text, 1);
        assert_eq!(
            found
                .as_ref()
                .map(|(path, _, query, _)| (path.as_str(), query.clone())),
            expected.map(|path| (path, None)),
            "{text}"
        );
    }
}

#[test]
fn a_query_does_not_hide_what_refuses_the_path_or_bound_its_length() {
    // The path part still names an `scp` host.
    assert_eq!(read("scp host:/p?x=1 now", 6), None);
    let long = format!("/a/b.html?x={} ok", "a".repeat(5000));
    assert_eq!(path::plain_path(&long, 1, false), None);
    let within = format!("/a/b.html?x={} ok", "a".repeat(4000));
    let found = path::plain_path(&within, 1, false).unwrap();
    assert_eq!(found.path, "/a/b.html");
    assert_eq!(found.query.map(str::len), Some(4003));
}

#[test]
fn a_query_wrapped_raw_onto_the_next_row_joins_its_path() {
    let wrapped = rows(&["see /tmp/m/index.html?sh", "eet=1 ok"], 24);
    let link = frame_link(&wrapped, 6, 0, 0, 24).unwrap();
    assert_eq!(
        link.target,
        RowTarget::Path {
            path: "/tmp/m/index.html".into(),
            line: None,
            query: Some("?sheet=1".into()),
            explicit: false,
            unjoined: None,
        }
    );
    assert_eq!(link.rows, vec![(0, 4..24), (1, 0..5)]);
}

/// What a one-row frame reads at `column` on a pane whose host writes
/// Windows paths when `windows_paths` is set.
fn read_on(text: &str, column: u16, windows_paths: bool) -> Option<Read> {
    let reading = LinkReading {
        raw_url_wraps: true,
        windows_paths,
    };
    let bounds = Bounds {
        columns: 0..60,
        rows: 0..1,
    };
    let link = reading.frame_link(&frame(text, 60, 1), column, 0, &bounds)?;
    let columns = columns(&link);
    match link.target {
        RowTarget::Path {
            path, line, query, ..
        } => Some((path, line, query, columns)),
        RowTarget::Web(_)
        | RowTarget::External(_)
        | RowTarget::OtherHost(_)
        | RowTarget::Malformed => None,
    }
}

#[test]
fn a_relative_windows_path_splits_its_query_on_a_windows_host() {
    assert_eq!(
        read_on(r"docs\index.html?x=1 ok", 1, true),
        split(r"docs\index.html", None, Some("?x=1"), 0..19)
    );
    assert_eq!(
        read_on(r"docs\guide.md#L10 ok", 1, true),
        split(r"docs\guide.md", Some(10), Some("#L10"), 0..17)
    );
}

#[test]
fn a_backslash_escapes_as_before_on_a_posix_host() {
    assert_eq!(
        read_on(r"/tmp/a\?b ok", 1, false),
        split("/tmp/a?b", None, None, 0..9)
    );
    assert_eq!(
        read_on(r"docs\index.html?x=1 ok", 1, false),
        split(r"docs\index.html?x=1", None, None, 0..19)
    );
}

/// A `file://` address of a page, and the path it names here.
#[cfg(unix)]
const PAGE: (&str, &str) = ("file:///tmp/m/index.html?sheet=1#top", "/tmp/m/index.html");
#[cfg(windows)]
const PAGE: (&str, &str) = ("file:///X:/m/index.html?sheet=1#top", r"X:\m\index.html");

#[test]
fn a_file_address_keeps_its_query_and_fragment_in_text_and_hyperlinks() {
    let (address, path) = PAGE;
    let page = |explicit| RowTarget::Path {
        path: path.into(),
        line: None,
        query: Some("?sheet=1#top".into()),
        explicit,
        unjoined: None,
    };
    let text = format!("open {address} ok");
    let plain = frame_link(&frame(&text, 60, 1), 6, 0, 0, 60).unwrap();
    assert_eq!(plain.target, page(false));
    assert_eq!(plain.rows, vec![(0, 5..5 + address.len() as u16)]);

    let mut s = surface("report");
    s.frame.cells[0].hyperlink = Some(0);
    let target =
        |s: &PaneSurfaceFrame| pane_link_at(s, 1., 1., 10., 20.).map(|link| link.link.target);
    s.frame.hyperlinks = vec![address.into()];
    assert_eq!(target(&s), Some(page(true)));
    // Without a scheme, the target is split as printed text is.
    s.frame.hyperlinks = vec!["docs/a.html?x=1".into()];
    assert_eq!(
        target(&s),
        Some(RowTarget::Path {
            path: "docs/a.html".into(),
            line: None,
            query: Some("?x=1".into()),
            explicit: true,
            unjoined: None,
        })
    );
    // A query longer than an address may carry is refused.
    let (base, _) = address.split_once('?').unwrap();
    s.frame.hyperlinks = vec![format!("{base}?x={}", "a".repeat(5000))];
    assert_eq!(target(&s), Some(RowTarget::Malformed));
    s.frame.hyperlinks = vec![format!("docs/a.html?x={}", "a".repeat(5000))];
    assert_eq!(target(&s), Some(RowTarget::Malformed));
}

/// A `file://` address of a source line, and the path it names here.
#[cfg(unix)]
const SOURCE: (&str, &str) = ("file:///tmp/m/main.rs#L10", "/tmp/m/main.rs");
#[cfg(windows)]
const SOURCE: (&str, &str) = ("file:///X:/m/main.rs#L10", r"X:\m\main.rs");

#[test]
fn a_file_address_or_hyperlink_takes_its_line_from_the_fragment() {
    let (address, path) = SOURCE;
    let source = |explicit| RowTarget::Path {
        path: path.into(),
        line: Some(10),
        query: Some("#L10".into()),
        explicit,
        unjoined: None,
    };
    let text = format!("open {address} ok");
    let plain = frame_link(&frame(&text, 60, 1), 6, 0, 0, 60).unwrap();
    assert_eq!(plain.target, source(false));

    let mut s = surface("report");
    s.frame.cells[0].hyperlink = Some(0);
    let target =
        |s: &PaneSurfaceFrame| pane_link_at(s, 1., 1., 10., 20.).map(|link| link.link.target);
    s.frame.hyperlinks = vec![address.into()];
    assert_eq!(target(&s), Some(source(true)));
    s.frame.hyperlinks = vec!["src/main.rs#L10-L20".into()];
    assert_eq!(
        target(&s),
        Some(RowTarget::Path {
            path: "src/main.rs".into(),
            line: Some(10),
            query: Some("#L10-L20".into()),
            explicit: true,
            unjoined: None,
        })
    );
}

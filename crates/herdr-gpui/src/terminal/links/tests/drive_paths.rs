use super::*;

fn read(text: &str, column: u16) -> Option<(String, Option<u32>, Range<u16>)> {
    frame_link(&frame(text, 60, 1), column, 0, 0, 60).and_then(|link| {
        match (columns(&link), link.target) {
            (columns, RowTarget::Path { path, line, .. }) => Some((path, line, columns)),
            (_, RowTarget::Web(_))
            | (_, RowTarget::External(_))
            | (_, RowTarget::OtherHost(_))
            | (_, RowTarget::Malformed) => None,
        }
    })
}

#[test]
fn a_drive_path_is_read_with_its_backslashes_kept() {
    for (text, path) in [
        (r"open X:\dev\something.html now", r"X:\dev\something.html"),
        (r"open x:/dev/something.html now", "x:/dev/something.html"),
        (
            r"open C:\repo\.tmp\#notes!.md now",
            r"C:\repo\.tmp\#notes!.md",
        ),
        (r"open C:\Users\me\ now", r"C:\Users\me\"),
        (r"open D:\ now", r"D:\"),
    ] {
        let start = 5;
        let end = start + path.chars().count() as u16;
        for column in [start, end - 1] {
            assert_eq!(
                read(text, column),
                Some((path.into(), None, start..end)),
                "{text} at {column}"
            );
        }
        assert!(read(text, end).is_none(), "{text}");
    }
}

#[test]
fn a_drive_path_keeps_its_location_and_drops_closing_punctuation() {
    assert_eq!(
        read(r"error at C:\src\main.rs:42:7: oops", 10),
        Some((r"C:\src\main.rs".into(), Some(42), 9..28))
    );
    assert_eq!(
        read(r"Saved to X:\out\report.pdf.", 10),
        Some((r"X:\out\report.pdf".into(), None, 9..26))
    );
}

#[test]
fn a_colon_that_is_no_drive_still_ends_the_path() {
    for (text, column) in [
        ("scp host:/path now", 6),
        (r"C:relative\file now", 1),
        (r"CD:\two\letters now", 1),
        (r"C:\a:b\c now", 1),
        (r"1:\digit now", 1),
    ] {
        assert!(read(text, column).is_none(), "{text}");
    }
}

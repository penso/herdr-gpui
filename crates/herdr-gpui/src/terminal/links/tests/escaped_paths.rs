use super::*;

fn located(text: &str, column: u16) -> Option<(String, Option<u32>, Range<u16>)> {
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
fn every_cell_of_a_pasted_path_links_to_the_original_name() {
    for name in [
        "/tmp/Shot 1 (it's).png",
        "/tmp/Shot, 1.png",
        "/tmp/Screenshot 2026-10-14 at 10.00.00\u{202f}AM.png",
        r"/tmp/a\b",
        "/tmp/[x]{y}<z>|w;v\"u`t",
        "/tmp/box\u{2500}name",
        "/tmp/wow!",
        "/tmp/$HOME~#^=*?&",
        "/tmp/build:debug.log",
        "/tmp/report:12",
        "/tmp/report.",
        "/tmp/dots..",
        "/tmp/a:b:3:4.",
        "README.",
    ] {
        // The exact text a file drop pastes.
        let mut text = String::from("see ");
        escape_into(name, &mut text);
        let link = 4..text.chars().count() as u16;
        text.push_str(" now");
        let width = text.chars().count() as u16;
        let row = frame(&text, width, 1);
        let read = |column| frame_link(&row, column, 0, 0, width);
        for column in link.clone() {
            let found = read(column).unwrap();
            assert_eq!(
                found.target,
                RowTarget::Path {
                    path: name.into(),
                    line: None,
                    query: None,
                    explicit: false,
                },
                "{text} at {column}"
            );
            assert_eq!(found.rows, vec![(0, link.clone())], "{text}");
        }
        assert!(read(link.end).is_none(), "{text}");
    }
}

#[test]
fn backslashes_unescape_only_before_shell_special_characters() {
    for (text, path) in [
        (r"/tmp/a\\b", r"/tmp/a\b"),
        (r"/tmp/a\b", r"/tmp/a\b"),
        (r"/tmp/wow\!", "/tmp/wow!"),
        (r"/tmp/x\ y:12.", "/tmp/x y"),
        (r"/tmp/a\:b:7", "/tmp/a:b"),
    ] {
        assert_eq!(
            located(text, 1).map(|(path, _, _)| path),
            Some(path.into()),
            "{text}"
        );
    }
    assert_eq!(located(r"/tmp/x\ y:12.", 1).unwrap().1, Some(12));
}

#[test]
fn an_unescaped_colon_still_names_a_location_or_a_scheme() {
    assert_eq!(
        located(r"/tmp/a\:12:3", 1).map(|(path, line, _)| (path, line)),
        Some(("/tmp/a:12".into(), Some(3)))
    );
    assert_eq!(located(r"/tmp/a\:b:7", 1).unwrap().1, Some(7));
    assert!(located("scp host:/path now", 6).is_none());
}

#[test]
fn the_size_estimate_matches_what_is_pasted() {
    for name in ["/tmp/plain", "/tmp/a b,c:d.", "/tmp/\u{202f}x?.", "."] {
        let mut text = String::new();
        escape_into(name, &mut text);
        assert_eq!(escaped_len(name), text.len(), "{name}");
    }
}

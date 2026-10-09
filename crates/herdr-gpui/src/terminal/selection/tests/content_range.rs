use super::*;

fn range(s: &PaneSurfaceFrame, selection: &Selection) -> Option<(String, TextRange)> {
    selection
        .content_range(s, CELL_WIDTH, CELL_HEIGHT)
        .map(|(pane, range)| (pane.to_owned(), range))
}

fn point(row: u32, col: u16) -> TextPoint {
    TextPoint { row, col }
}

/// What a plugin action is sent: the cells the highlight covers, inclusive,
/// whichever way it was dragged, cut at the half-cell the pointer stopped in.
#[test]
fn a_visible_selection_converts_to_inclusive_cells_either_way_it_was_dragged() {
    let s = surface("abcdefghij");
    // Left half of column 1 to the right half of column 3 on row 1.
    let forward = drag(&s, (11., 21.), (38., 21.));
    let expected = Some((
        "pane".to_owned(),
        TextRange {
            start: point(1, 1),
            end: point(1, 3),
        },
    ));
    assert_eq!(range(&s, &forward), expected);
    let backward = drag(&s, (38., 21.), (11., 21.));
    assert_eq!(range(&s, &backward), expected);
    // Stopping in the left half of column 3 leaves that cell out, and a
    // drag that never left its half-cell selects nothing at all.
    let short = drag(&s, (11., 21.), (32., 21.));
    assert_eq!(range(&s, &short).unwrap().1.end, point(1, 2));
    let click = drag(&s, (11., 21.), (13., 21.));
    assert_eq!(range(&s, &click), None);
    // Visible rows are not "offscreen": the copy path keeps reading cells.
    assert!(
        forward
            .offscreen_range(&s, CELL_WIDTH, CELL_HEIGHT)
            .is_none()
    );
}

/// Columns are counted from the pane's own left edge and rows from the top
/// of its retained buffer, as `pane.selection.read` takes them.
#[test]
fn content_ranges_are_pane_relative_and_absolute_in_the_buffer() {
    let mut s = scrolled("abcdefghij", 4);
    s.frame = frame("abcdefghij", 14, 3);
    let pane = &mut s.panes[0];
    pane.rect.x = 4;
    pane.inner_rect.x = 4;
    let selection = drag(&s, (61., 1.), (98., 41.));
    assert_eq!(
        range(&s, &selection),
        Some((
            "pane".to_owned(),
            TextRange {
                start: point(4, 2),
                end: point(6, 5),
            },
        ))
    );
}

/// A popup selection belongs to the popup's terminal, never to the pane it
/// covers, and a pane the surface no longer paints has no range.
#[test]
fn popup_and_vanished_pane_selections_have_no_content_range() {
    let mut s = surface("abcdefghij");
    let pane = drag(&s, (11., 21.), (38., 21.));
    s.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "popup".into(),
        title: String::new(),
        width: None,
        height: None,
        frame: frame("popup", 4, 2),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 40,
        pixel_height: 40,
    }));
    let popup = drag(&s, (31., 11.), (66., 31.));
    assert!(popup.in_popup("popup"));
    assert_eq!(range(&s, &popup), None);
    assert_eq!(range(&s, &pane), None, "the popup covers the pane");
    s.popup = None;
    s.panes.clear();
    assert_eq!(range(&s, &pane), None);
}

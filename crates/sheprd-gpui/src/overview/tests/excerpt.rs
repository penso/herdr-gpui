use crate::{overview::excerpt::of_pane, palette::LastSeen};
use sheprd_client::protocol::{
    CellData, ClientShellSnapshot, FrameData, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect,
};
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

const WIDTH: u16 = 12;

fn cell(symbol: &str) -> CellData {
    CellData {
        symbol: symbol.into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    }
}

/// A surface of `lines`, one pane of `pane` inset a column from the frame's
/// edge, as a border leaves it.
fn surface(boot: &str, pane: &str, lines: &[&str]) -> Arc<PaneSurfaceFrame> {
    let height = lines.len() as u16;
    let mut cells = Vec::new();
    for line in lines {
        let mut row: Vec<CellData> = line.chars().map(|c| cell(&c.to_string())).collect();
        row.resize(usize::from(WIDTH), cell(""));
        cells.extend(row);
    }
    let rect = SurfaceRect {
        x: 1,
        y: 0,
        width: WIDTH - 2,
        height,
    };
    let shown = PaneSurfacePane {
        pane_id: pane.into(),
        content_revision: 1,
        rect,
        inner_rect: rect,
        scrollbar_rect: None,
        scroll: None,
        focused: true,
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        alternate_screen_active: false,
        pixel_width: 0,
        pixel_height: 0,
    };
    Arc::new(PaneSurfaceFrame {
        boot_id: boot.into(),
        projection_revision: 1,
        surface_revision: 1,
        frame: FrameData {
            width: WIDTH,
            height,
            cells,
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![shown],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    })
}

/// The fixture's snapshot, with `boot`, showing the tab of pane `w1:p1`.
fn snapshot(boot: &str) -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    snapshot.boot_id = boot.into();
    snapshot
}

#[test]
fn it_keeps_the_last_lines_that_say_something_in_order() {
    let live = surface(
        "b",
        "p",
        &[
            " one",
            " two",
            " Run rm?",
            " ──────",
            " 1. Yes",
            "",
            " 2. No",
        ],
    );
    let lines = of_pane(Some(&live), &LastSeen::default(), "b", "p", None).unwrap();
    assert_eq!(lines, ["Run rm?", "1. Yes", "2. No"]);
}

#[test]
fn it_reads_only_inside_the_pane() {
    // Columns 0 and 11 are the border, outside the pane's inner rectangle.
    let live = surface("b", "p", &["|abcdefghij|"]);
    let lines = of_pane(Some(&live), &LastSeen::default(), "b", "p", None).unwrap();
    assert_eq!(lines, ["abcdefghij"]);
}

#[test]
fn a_wide_characters_continuation_cell_adds_nothing() {
    let mut frame = (*surface("b", "p", &[" x"])).clone();
    frame.frame.cells[1] = cell("界");
    frame.frame.cells[2] = CellData {
        skip: true,
        ..cell("")
    };
    frame.frame.cells[3] = cell("x");
    let live = Arc::new(frame);
    let lines = of_pane(Some(&live), &LastSeen::default(), "b", "p", None).unwrap();
    assert_eq!(lines, ["界x"]);
}

#[test]
fn terminal_text_is_cleaned_before_it_is_drawn() {
    let mut frame = (*surface("b", "p", &[" abc"])).clone();
    frame.frame.cells[2] = cell("\u{1b}");
    frame.frame.cells[3] = cell("\u{202e}");
    let live = Arc::new(frame);
    let lines = of_pane(Some(&live), &LastSeen::default(), "b", "p", None).unwrap();
    assert_eq!(lines, ["a"]);
}

#[test]
fn a_pane_with_nothing_to_say_has_no_excerpt() {
    let live = surface("b", "p", &["", " ─────", " ╭───╮"]);
    assert_eq!(
        of_pane(Some(&live), &LastSeen::default(), "b", "p", None),
        None
    );
}

#[test]
fn another_boots_cells_and_other_panes_are_not_read() {
    let live = surface("old", "p", &[" hello"]);
    let seen = LastSeen::default();
    assert_eq!(of_pane(Some(&live), &seen, "new", "p", None), None);
    let live = surface("new", "other", &[" hello"]);
    assert_eq!(of_pane(Some(&live), &seen, "new", "p", None), None);
}

#[test]
fn a_frame_shorter_than_its_pane_claims_is_not_read_past_its_end() {
    let mut frame = (*surface("b", "p", &[" one", " two"])).clone();
    frame.frame.cells.truncate(usize::from(WIDTH) + 2);
    let live = Arc::new(frame);
    let lines = of_pane(Some(&live), &LastSeen::default(), "b", "p", None).unwrap();
    assert_eq!(lines, ["one"]);
}

/// Remembers `surface`, left at `left` seconds, as its host moving on would.
fn remembered(surface: &Arc<PaneSurfaceFrame>, left: u64) -> LastSeen {
    let before = snapshot("b");
    let mut after = before.clone();
    let mut other = after.tabs[0].clone();
    other.tab_id = "w1:other".into();
    after.tabs.push(other);
    after.focused_tab_id = Some("w1:other".into());
    let mut seen = LastSeen::default();
    seen.observe((Some(&before), Some(surface)), (Some(&after), None), left);
    seen
}

#[test]
fn a_frame_left_after_the_agent_blocked_shows_its_question() {
    let frame = surface("b", "w1:p1", &[" Allow?"]);
    let seen = remembered(&frame, 100);
    assert_eq!(seen.frames("b").count(), 1, "the frame was remembered");
    let since = UNIX_EPOCH + Duration::from_secs(90);
    assert_eq!(
        of_pane(None, &seen, "b", "w1:p1", Some(since)).unwrap(),
        ["Allow?"]
    );
}

#[test]
fn a_frame_from_before_the_agent_blocked_is_not_its_question() {
    let frame = surface("b", "w1:p1", &[" Old text"]);
    let seen = remembered(&frame, 100);
    let since = UNIX_EPOCH + Duration::from_secs(101);
    assert_eq!(of_pane(None, &seen, "b", "w1:p1", Some(since)), None);
    // With no time to compare, nothing says the frame is current.
    assert_eq!(of_pane(None, &seen, "b", "w1:p1", None), None);
}

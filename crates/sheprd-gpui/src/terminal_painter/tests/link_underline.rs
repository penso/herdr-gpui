use super::*;
use crate::terminal_painter::decorations::decoration_spans;

const CELL_WIDTH: f32 = 8.;

fn frame(cells: Vec<CellData>) -> FrameData {
    FrameData {
        width: cells.len() as u16,
        height: 1,
        cells,
        cursor: None,
        hyperlinks: vec!["https://example.com/".into()],
        graphics: Vec::new(),
    }
}

fn linked(symbol: &str) -> CellData {
    CellData {
        hyperlink: Some(0),
        ..cell(symbol)
    }
}

fn spans(frame: &FrameData, index: usize) -> Vec<(f32, f32, f32)> {
    decoration_spans(frame, index, CELL_WIDTH, CELL_HEIGHT).collect()
}

#[test]
fn a_hyperlink_is_underlined_with_dashes_at_rest() {
    let frame = frame(vec![cell("a"), linked("b"), linked(" "), cell("c")]);
    assert_eq!(spans(&frame, 0), vec![]);
    assert_eq!(spans(&frame, 3), vec![]);
    // A dash is 4 pixels and a gap 3, counted from the grid's left edge, so
    // the dashes of the two linked cells continue one another.
    assert_eq!(spans(&frame, 1), vec![(18., 8., 11.), (18., 14., 16.)]);
    assert_eq!(spans(&frame, 2), vec![(18., 16., 18.), (18., 21., 24.)]);
}

#[test]
fn the_dashes_sit_where_an_underline_would() {
    let mut underlined = cell("a");
    underlined.modifier = UNDERLINE;
    let frame = frame(vec![underlined, linked("b")]);
    let [(solid, ..)] = spans(&frame, 0)[..] else {
        panic!("one underline");
    };
    assert!(spans(&frame, 1).iter().all(|(y, ..)| *y == solid));
}

#[test]
fn a_linked_cell_the_program_underlined_is_dashed_not_solid() {
    let mut both = linked("a");
    both.modifier = UNDERLINE | STRIKETHROUGH;
    let frame = frame(vec![both]);
    // The strikethrough stays whole; the underline is the link's dashes.
    assert_eq!(
        spans(&frame, 0),
        vec![(10., 0., 8.), (18., 0., 4.), (18., 7., 8.)]
    );
}

#[test]
fn a_wide_glyph_continuation_takes_its_glyphs_link() {
    let continuation = CellData {
        skip: true,
        ..cell("")
    };
    let frame = frame(vec![
        linked("好"),
        continuation.clone(),
        cell("好"),
        continuation,
    ]);
    assert!(!spans(&frame, 1).is_empty());
    assert_eq!(spans(&frame, 3), vec![]);
}

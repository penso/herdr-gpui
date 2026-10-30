use crate::core_terminals::selection::{Cell, Selection};
use herdr_backend::FrameData;
use herdr_client::protocol::CellData;

/// A frame showing `rows`, each padded with blanks to `width`.
fn frame(width: u16, rows: &[&str]) -> FrameData {
    let cells = rows
        .iter()
        .flat_map(|row| {
            let mut symbols: Vec<String> = row.chars().map(String::from).collect();
            symbols.resize(usize::from(width), " ".into());
            symbols
        })
        .map(|symbol| CellData {
            symbol,
            fg: 0,
            bg: 0,
            modifier: 0,
            skip: false,
            hyperlink: None,
        })
        .collect();
    FrameData {
        cells,
        width,
        height: u16::try_from(rows.len()).unwrap(),
        cursor: None,
        hyperlinks: Vec::new(),
        graphics: Vec::new(),
    }
}

fn at(row: u16, column: u16) -> Cell {
    Cell { row, column }
}

#[test]
fn a_press_that_never_moves_selects_nothing() {
    let shown = frame(8, &["hello"]);
    let selection = Selection::at(at(0, 2));
    assert!(selection.is_empty());
    assert!(selection.rows(8).is_empty());
    assert_eq!(selection.text(&shown), "");
}

#[test]
fn a_drag_selects_to_the_ends_of_rows_between_and_reads_either_way() {
    let shown = frame(8, &["ab cd", "efgh", "ij kl"]);
    let mut forward = Selection::at(at(0, 3));
    forward.extend(at(2, 1));
    assert_eq!(forward.rows(8), [(0, 3..8), (1, 0..8), (2, 0..2)]);
    assert_eq!(forward.text(&shown), "cd\nefgh\nij");
    let mut backward = Selection::at(at(2, 1));
    backward.extend(at(0, 3));
    assert_eq!(backward.text(&shown), forward.text(&shown));
}

#[test]
fn double_and_triple_clicks_take_a_word_and_a_line() {
    let shown = frame(16, &["git push origin"]);
    assert_eq!(Selection::word(&shown, at(0, 6)).text(&shown), "push");
    assert_eq!(Selection::word(&shown, at(0, 0)).text(&shown), "git");
    assert!(Selection::word(&shown, at(0, 3)).is_empty());
    assert_eq!(
        Selection::line(&shown, at(0, 6)).text(&shown),
        "git push origin"
    );
}

#[test]
fn a_wide_character_is_copied_once() {
    // The second cell stands in for the wide character's right half.
    let mut shown = frame(4, &["界x a"]);
    shown.cells[1].skip = true;
    shown.cells[1].symbol = String::new();
    let mut selection = Selection::at(at(0, 0));
    selection.extend(at(0, 3));
    assert_eq!(selection.text(&shown), "界 a");
}

#[test]
fn a_one_letter_word_or_line_is_still_selected() {
    let shown = frame(3, &["x y"]);
    let word = Selection::word(&shown, at(0, 0));
    assert!(!word.is_empty());
    assert_eq!(word.text(&shown), "x");
    let narrow = frame(1, &["z"]);
    assert_eq!(Selection::line(&narrow, at(0, 0)).text(&narrow), "z");
}

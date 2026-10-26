use super::*;

#[test]
fn drawn_and_source_offsets_round_trip_through_tabs_and_dropped_controls() {
    let raw = "a\tb\u{202e}c";
    let code = CodeText::expand(raw);
    assert_eq!(code.text, "a   bc");
    assert_eq!(code.end, raw.len());
    assert_eq!(code.shown(1), Some(1));
    assert_eq!(code.shown(2), Some(4));
    // A caret inside the tab's spaces stands for the tab.
    assert_eq!(code.source(2), 1);
    // A range ending inside the tab copies all of it.
    assert_eq!(code.source_end(2), 2);
    assert_eq!(code.source_end(code.text.len()), code.end);
}

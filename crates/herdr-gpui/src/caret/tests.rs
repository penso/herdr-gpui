use super::*;
use gpui::{FontId, GlyphId, ShapedGlyph, ShapedRun, point};

/// `text` laid out as a bidi shaper draws it: each character 10px wide, at
/// the place `visual` gives its byte index from the left.
fn line(text: &str, visual: &[usize]) -> LineLayout {
    let glyphs = visual
        .iter()
        .enumerate()
        .map(|(k, &index)| ShapedGlyph {
            id: GlyphId(index as u32),
            position: point(px(10. * k as f32), px(0.)),
            index,
            is_emoji: false,
        })
        .collect();
    LineLayout {
        width: px(10. * visual.len() as f32),
        runs: vec![ShapedRun {
            font_id: FontId(0),
            glyphs,
        }],
        len: text.len(),
        ..Default::default()
    }
}

fn xs(carets: &Carets, indices: &[usize]) -> Vec<f32> {
    indices
        .iter()
        .map(|&index| f32::from(carets.x(index)))
        .collect()
}

fn spans(carets: &Carets, range: Range<usize>) -> Vec<(f32, f32)> {
    carets
        .spans(range)
        .into_iter()
        .map(|span| (f32::from(span.start), f32::from(span.end)))
        .collect()
}

#[test]
fn left_to_right_text_matches_gpui() {
    let text = "abc";
    let layout = line(text, &[0, 1, 2]);
    let carets = Carets::new(text, &layout);
    for index in 0..=text.len() {
        assert_eq!(carets.x(index), layout.x_for_index(index), "{index}");
    }
    assert_eq!(carets.index_at(px(14.)), 1);
    assert_eq!(carets.index_at(px(16.)), 2);
    assert_eq!(spans(&carets, 1..3), [(10., 30.)]);
}

/// "ab אב cd": Latin, a Hebrew word drawn backwards, then Latin again.
const MIXED: &str = "ab \u{5d0}\u{5d1} cd";
/// a b ␠ ב א ␠ c d, by byte index.
const MIXED_VISUAL: [usize; 8] = [0, 1, 2, 5, 3, 7, 8, 9];

#[test]
fn the_caret_moves_through_a_hebrew_word_from_its_right() {
    let carets = Carets::new(MIXED, &line(MIXED, &MIXED_VISUAL));
    // Before א is its right edge, between א and ב their shared edge, and
    // the space after the word starts at its left again.
    assert_eq!(
        xs(&carets, &[0, 1, 2, 3, 5, 7, 8, 9, 10]),
        [0., 10., 20., 50., 40., 50., 60., 70., 80.]
    );
    // GPUI's own answer leaves the caret at the word's left edge for both.
    let layout = line(MIXED, &MIXED_VISUAL);
    assert_eq!(layout.x_for_index(3), layout.x_for_index(5));
}

#[test]
fn a_click_lands_on_the_nearer_side_of_a_hebrew_letter() {
    let carets = Carets::new(MIXED, &line(MIXED, &MIXED_VISUAL));
    // א is drawn at 40..50: its right half is before it, its left half after.
    assert_eq!(carets.index_at(px(47.)), 3);
    assert_eq!(carets.index_at(px(42.)), 5);
    // ב at 30..40 likewise.
    assert_eq!(carets.index_at(px(38.)), 5);
    assert_eq!(carets.index_at(px(31.)), 7);
    // Beyond either end.
    assert_eq!(carets.index_at(px(-5.)), 0);
    assert_eq!(carets.index_at(px(500.)), MIXED.len());
}

#[test]
fn a_selection_across_a_direction_change_is_two_stretches() {
    let carets = Carets::new(MIXED, &line(MIXED, &MIXED_VISUAL));
    // The whole Hebrew word is one stretch.
    assert_eq!(spans(&carets, 3..7), [(30., 50.)]);
    // From ב into "c": ב on one side, the space and c on the other.
    assert_eq!(spans(&carets, 5..9), [(30., 40.), (50., 70.)]);
    // The input method's box covers both.
    let extent = carets.extent(5..9);
    assert_eq!((f32::from(extent.start), f32::from(extent.end)), (30., 70.));
    // An empty range is the caret.
    let caret = carets.extent(5..5);
    assert_eq!(caret.start, caret.end);
    assert_eq!(f32::from(caret.start), 40.);
}

#[test]
fn a_hebrew_paragraph_ends_on_the_left() {
    // "אב cd": the first strong letter is Hebrew, so the paragraph reads
    // right to left and draws c d ␠ ב א.
    let text = "\u{5d0}\u{5d1} cd";
    let carets = Carets::new(text, &line(text, &[5, 6, 4, 2, 0]));
    // Typing Hebrew at the start puts the caret at the right edge.
    assert_eq!(f32::from(carets.x(0)), 50.);
    assert_eq!(f32::from(carets.x(2)), 40.);
    // The space reads with the Hebrew around it; "cd" reads left to right.
    assert_eq!(f32::from(carets.x(4)), 30.);
    assert_eq!(f32::from(carets.x(6)), 10.);
    assert_eq!(f32::from(carets.x(text.len())), 20.);
    // Typing more Hebrew after "אב" alone goes on to the left.
    let word = "\u{5d0}\u{5d1}";
    let carets = Carets::new(word, &line(word, &[2, 0]));
    assert_eq!(f32::from(carets.x(word.len())), 0.);
}

#[test]
fn a_ligature_shares_its_width_among_its_characters() {
    // "ffi" drawn as one 30px glyph after "a".
    let text = "affi";
    let layout = LineLayout {
        width: px(40.),
        runs: vec![ShapedRun {
            font_id: FontId(0),
            glyphs: [(0., 0), (10., 1)]
                .into_iter()
                .map(|(x, index)| ShapedGlyph {
                    id: GlyphId(index as u32),
                    position: point(px(x), px(0.)),
                    index,
                    is_emoji: false,
                })
                .collect(),
        }],
        len: text.len(),
        ..Default::default()
    };
    let carets = Carets::new(text, &layout);
    assert_eq!(xs(&carets, &[1, 2, 3, 4]), [10., 20., 30., 40.]);
}

#[test]
fn an_empty_line_puts_the_caret_at_its_start() {
    let carets = Carets::new("", &LineLayout::default());
    assert_eq!(carets.x(0), px(0.));
    assert_eq!(carets.index_at(px(30.)), 0);
    assert!(carets.spans(0..0).is_empty());
}

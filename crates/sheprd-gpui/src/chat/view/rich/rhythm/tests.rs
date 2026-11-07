use super::*;
use crate::chat::rich::Block;

fn block(kind: Kind) -> Block {
    Block::Paragraph {
        kind,
        indent: 0,
        content: Vec::new(),
    }
}

#[test]
fn the_first_block_starts_flush() {
    assert_eq!(
        space_before(None, &block(Kind::Heading(HeadingLevel::H1))),
        0.
    );
}

#[test]
fn a_heading_binds_to_its_text_not_what_came_before() {
    let heading = block(Kind::Heading(HeadingLevel::H2));
    let paragraph = block(Kind::Paragraph);
    let above = space_before(Some(&paragraph), &heading);
    let below = space_before(Some(&heading), &paragraph);
    assert!(above > 2. * below, "{above} above, {below} below");
}

#[test]
fn list_items_sit_closer_than_paragraphs() {
    let item = block(Kind::Bullet);
    let paragraph = block(Kind::Paragraph);
    let between_items = space_before(Some(&item), &block(Kind::Numbered(2)));
    let between_paragraphs = space_before(Some(&paragraph), &paragraph);
    assert!(between_items < between_paragraphs);
    // A list still stands apart from the prose around it.
    assert_eq!(space_before(Some(&paragraph), &item), between_paragraphs);
    assert_eq!(space_before(Some(&item), &paragraph), between_paragraphs);
}

#[test]
fn headings_stay_near_the_text_size_and_shrink_by_level() {
    let scales = [
        HeadingLevel::H1,
        HeadingLevel::H2,
        HeadingLevel::H3,
        HeadingLevel::H4,
    ]
    .map(heading_scale);
    assert!(scales.windows(2).all(|pair| pair[0] >= pair[1]));
    assert!(scales[0] <= 1.4);
    assert_eq!(scales[3], 1.);
}

#[test]
fn prose_keeps_to_the_measure_but_rules_do_not() {
    assert!(is_prose(&block(Kind::Paragraph)));
    assert!(is_prose(&block(Kind::Quote)));
    assert!(!is_prose(&block(Kind::Rule)));
}

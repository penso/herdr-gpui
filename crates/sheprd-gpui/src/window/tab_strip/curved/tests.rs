#![allow(clippy::unwrap_used)]

use super::{Connected, Side, Span, body_path, card_bounds, eased, fillets, margins};
use gpui::{Bounds, Pixels, point, px, size};

const SHAPE: Connected = Connected {
    top: 5.,
    radius: 8.,
    shoulder: 10.,
    inset: 6.,
    card_radius: 10.,
};

fn content() -> Bounds<Pixels> {
    Bounds::new(point(px(100.), px(40.)), size(px(400.), px(300.)))
}

#[test]
fn corners_square_off_where_the_body_reaches_them() {
    assert_eq!(eased(0., 10.), 0.);
    assert_eq!(eased(-4., 10.), 0.);
    assert_eq!(eased(10., 10.), 10.);
    assert_eq!(eased(400., 10.), 10.);
    let half = eased(5., 10.);
    assert!(half > 0. && half < 10.);
}

#[test]
fn the_card_keeps_chrome_on_every_side_but_the_strips() {
    let bounds = content();
    let above = card_bounds(bounds, &SHAPE, Some(Side::Above));
    assert_eq!(above.top(), bounds.top());
    assert_eq!(above.bottom(), bounds.bottom() - px(6.));
    assert_eq!(above.left(), bounds.left() + px(6.));
    assert_eq!(above.right(), bounds.right() - px(6.));
    let below = card_bounds(bounds, &SHAPE, Some(Side::Below));
    assert_eq!(below.top(), bounds.top() + px(6.));
    assert_eq!(below.bottom(), bounds.bottom());
    let alone = card_bounds(bounds, &SHAPE, None);
    assert_eq!(alone.top(), bounds.top() + px(6.));
    assert_eq!(alone.bottom(), bounds.bottom() - px(6.));
}

#[test]
fn the_chrome_covers_only_what_lies_outside_the_card() {
    let bounds = content();
    let card = card_bounds(bounds, &SHAPE, Some(Side::Above));
    let pieces = margins(bounds, card);
    // Left, right, and bottom; no band on the strip's side.
    assert_eq!(pieces.len(), 3);
    let area: f32 = pieces
        .iter()
        .map(|piece| f32::from(piece.size.width) * f32::from(piece.size.height))
        .sum();
    let card_area = f32::from(card.size.width) * f32::from(card.size.height);
    let whole = f32::from(bounds.size.width) * f32::from(bounds.size.height);
    assert!((area + card_area - whole).abs() < 0.5);
    assert!(pieces.iter().all(|piece| !piece.intersects(&card)));
}

#[test]
fn square_corners_draw_no_fillet() {
    let card = card_bounds(content(), &SHAPE, Some(Side::Above));
    let drawn = fillets(card, [0., 10., 10., 10.]);
    assert!(drawn[0].is_none());
    assert!(drawn[1..].iter().all(Option::is_some));
}

#[test]
fn the_body_flares_its_shoulders_past_the_tab_toward_the_content() {
    let strip = Bounds::new(point(px(0.), px(0.)), size(px(600.), px(38.)));
    let span = Span {
        left: 100.,
        width: 160.,
    };
    let above = body_path(strip, span, &SHAPE, Side::Above).unwrap();
    assert_eq!(above.bounds.left(), px(90.));
    assert_eq!(above.bounds.right(), px(270.));
    assert_eq!(above.bounds.top(), px(5.));
    assert_eq!(above.bounds.bottom(), px(38.));
    let below = body_path(strip, span, &SHAPE, Side::Below).unwrap();
    assert_eq!(below.bounds.top(), px(0.));
    assert_eq!(below.bounds.bottom(), px(33.));
}

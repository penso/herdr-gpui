use super::*;
use core::prelude::v1::test;

fn row(top: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(top)), size(px(100.), px(height)))
}

#[test]
fn a_row_in_view_stays_and_one_outside_moves_just_into_it() {
    let (top, bottom) = (px(100.), px(500.));
    assert_eq!(reveal_shift(row(200., 20.), top, bottom), px(0.));
    // Below: up until its bottom edge meets the view's.
    assert_eq!(reveal_shift(row(600., 20.), top, bottom), px(-120.));
    // Above, or under a pinned header: down until its top shows.
    assert_eq!(reveal_shift(row(40., 20.), top, bottom), px(60.));
    // Taller than the view: its top wins.
    assert_eq!(reveal_shift(row(600., 900.), top, bottom), px(-500.));
}

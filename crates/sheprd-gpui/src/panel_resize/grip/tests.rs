use super::*;
use core::prelude::v1::test;

#[test]
fn the_pill_starts_still_and_grows_with_each_level() {
    let start = Instant::now();
    let mut pill = Indicator::default();
    assert_eq!(pill.look(start), (0., 0.));
    assert!(!pill.moving(start));
    let mut last = 0.;
    for (step, level) in [Level::Hovered, Level::Pressed, Level::Dragging]
        .into_iter()
        .enumerate()
    {
        let at = start + motion::ENTER * (step as u32 + 1) * 2;
        pill.set(level, at);
        assert!(pill.moving(at));
        assert_eq!(pill.look(at).0, last, "{level:?} starts where it was");
        let (length, opacity) = pill.look(at + motion::ENTER);
        assert_eq!((length, opacity), level.look());
        assert!(length > last, "{level:?}");
        last = length;
    }
}

#[test]
fn a_release_mid_way_turns_back_from_where_the_pill_is() {
    let start = Instant::now();
    let mut pill = Indicator::default();
    pill.set(Level::Hovered, start);
    let mid = start + motion::ENTER / 4;
    let (length, _) = pill.look(mid);
    assert!(length > 0. && length < 20., "{length}");
    pill.set(Level::Idle, mid);
    assert_eq!(pill.look(mid).0, length);
    assert_eq!(pill.look(mid + motion::ENTER), (0., 0.));
    assert!(!pill.moving(mid + motion::ENTER));
}

#[test]
fn only_this_band_s_drag_makes_it_dragging() {
    let mut hold = Hold::default();
    assert_eq!(hold.level(true, false), Level::Idle, "another band's drag");
    hold.hover(0, true);
    assert_eq!(hold.level(true, false), Level::Hovered);
    hold.pressed = true;
    assert_eq!(hold.level(false, false), Level::Pressed);
    assert_eq!(hold.level(true, false), Level::Dragging);
    hold.hover(0, false);
    assert_eq!(
        hold.level(true, false),
        Level::Dragging,
        "the pointer left the band"
    );
    // An owner tracking its own drag says so, pressed or not.
    assert_eq!(Hold::default().level(false, true), Level::Dragging);
}

#[test]
fn an_edge_in_pieces_stays_hovered_crossing_between_them() {
    let mut hold = Hold::default();
    hold.hover(0, true);
    // Entering the next piece may arrive before leaving the last.
    hold.hover(1, true);
    hold.hover(0, false);
    assert_eq!(hold.level(false, false), Level::Hovered);
    hold.hover(1, false);
    assert_eq!(hold.level(false, false), Level::Idle);
}

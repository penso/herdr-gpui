use super::*;
use crate::motion::LEAVE;
use gpui::point;

fn live() -> Appear {
    Appear {
        live: true,
        ..Default::default()
    }
}

#[test]
fn a_popover_slides_in_toward_its_anchor() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Popover(point(px(10.), px(20.)))), start);
    assert_eq!(appear.panel(start), 0.);
    assert_eq!(appear.offset(false, start), px(POPOVER_SLIDE));
    assert!(appear.moving(start));
    // A popover never dims the window.
    assert_eq!(appear.backdrop(start), 0.);
    let done = start + ENTER;
    assert_eq!(appear.panel(done), 1.);
    assert_eq!(appear.offset(false, done), px(0.));
    assert!(!appear.moving(done));
}

#[test]
fn a_popover_opened_elsewhere_appears_again() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Popover(point(px(10.), px(20.)))), start);
    let later = start + ENTER;
    appear.observe(Some(Shown::Popover(point(px(10.), px(20.)))), later);
    assert_eq!(appear.panel(later), 1.);
    appear.observe(Some(Shown::Popover(point(px(80.), px(20.)))), later);
    assert_eq!(appear.panel(later), 0.);
}

#[test]
fn a_dialog_rises_over_dimming_that_lifts_after_it() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Dialog), start);
    assert_eq!(appear.offset(true, start), px(DIALOG_RISE));
    assert_eq!(appear.backdrop(start), 0.);
    let open = start + ENTER;
    // Another page of the same dialog, such as an Add Device source, stays put.
    appear.observe(Some(Shown::Dialog), open);
    assert_eq!(appear.panel(open), 1.);
    assert_eq!(appear.backdrop(open), 1.);
    appear.observe(None, open);
    assert_eq!(appear.backdrop(open), 1.);
    assert!(appear.moving(open));
    assert_eq!(appear.backdrop(open + LEAVE), 0.);
    assert!(!appear.moving(open + LEAVE));
}

#[test]
fn a_dialog_closed_midway_lifts_from_where_it_was() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Dialog), start);
    let midway = start + ENTER / 3;
    let shown = appear.backdrop(midway);
    appear.observe(None, midway);
    assert!((appear.backdrop(midway) - shown).abs() < 0.01);
}

#[test]
fn unit_tests_see_menus_settled() {
    let start = Instant::now();
    let mut appear = Appear::default();
    appear.observe(Some(Shown::Dialog), start);
    assert_eq!(appear.panel(start), 1.);
    assert_eq!(appear.backdrop(start), 1.);
    assert!(!appear.moving(start));
    appear.observe(None, start);
    assert_eq!(appear.backdrop(start), 0.);
}

#[test]
fn the_dimming_keeps_its_color_and_scales_its_alpha() {
    assert_eq!(backdrop(0x112233, 1.), rgba(0x112233b0));
    assert_eq!(backdrop(0x112233, 0.), rgba(0x11223300));
    assert_eq!(backdrop(0x112233, 2.), rgba(0x112233b0));
}

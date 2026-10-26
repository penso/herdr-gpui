use super::*;
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
    appear.observe(Some(Shown::Popover(point(px(10.), px(20.)))), None, start);
    assert_eq!(appear.panel(start), 0.);
    assert_eq!(
        Appear::offset(false, appear.panel(start)),
        px(POPOVER_SLIDE)
    );
    assert!(appear.moving(start));
    // A popover never dims the window.
    assert_eq!(appear.backdrop(start), 0.);
    let done = start + ENTER;
    assert_eq!(appear.panel(done), 1.);
    assert_eq!(Appear::offset(false, appear.panel(done)), px(0.));
    assert!(!appear.moving(done));
}

#[test]
fn a_popover_opened_elsewhere_appears_again() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Popover(point(px(10.), px(20.)))), None, start);
    let later = start + ENTER;
    appear.observe(Some(Shown::Popover(point(px(10.), px(20.)))), None, later);
    assert_eq!(appear.panel(later), 1.);
    appear.observe(Some(Shown::Popover(point(px(80.), px(20.)))), None, later);
    assert_eq!(appear.panel(later), 0.);
}

#[test]
fn a_dialog_rises_over_dimming_that_lifts_after_it() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Dialog), None, start);
    assert_eq!(Appear::offset(true, appear.panel(start)), px(DIALOG_RISE));
    assert_eq!(appear.backdrop(start), 0.);
    let open = start + ENTER;
    // Another page of the same dialog, such as an Add Device source, stays put.
    appear.observe(Some(Shown::Dialog), None, open);
    assert_eq!(appear.panel(open), 1.);
    assert_eq!(appear.backdrop(open), 1.);
    appear.observe(None, None, open);
    assert_eq!(appear.backdrop(open), 1.);
    assert!(appear.moving(open));
    assert_eq!(appear.backdrop(open + LEAVE), 0.);
    assert!(!appear.moving(open + LEAVE));
}

#[test]
fn a_dialog_closed_midway_lifts_from_where_it_was() {
    let start = Instant::now();
    let mut appear = live();
    appear.observe(Some(Shown::Dialog), None, start);
    let midway = start + ENTER / 3;
    let shown = appear.backdrop(midway);
    appear.observe(None, None, midway);
    assert!((appear.backdrop(midway) - shown).abs() < 0.01);
}

#[test]
fn unit_tests_see_menus_settled() {
    let start = Instant::now();
    let mut appear = Appear::default();
    appear.observe(Some(Shown::Dialog), None, start);
    assert_eq!(appear.panel(start), 1.);
    assert_eq!(appear.backdrop(start), 1.);
    assert!(!appear.moving(start));
    appear.observe(None, None, start);
    assert_eq!(appear.backdrop(start), 0.);
}

#[test]
fn the_dimming_keeps_its_color_and_scales_its_alpha() {
    assert_eq!(backdrop(0x112233, 1.), rgba(0x112233b0));
    assert_eq!(backdrop(0x112233, 0.), rgba(0x11223300));
    assert_eq!(backdrop(0x112233, 2.), rgba(0x112233b0));
}

#[test]
fn all_devices_sinks_away_after_it_closes() {
    let start = Instant::now();
    let mut appear = live();
    let anchor = Shown::Popover(point(px(10.), px(600.)));
    appear.observe(Some(anchor), Some(Page::Devices), start);
    let open = start + ENTER;
    appear.observe(Some(anchor), Some(Page::Devices), open);
    assert!(appear.leaving(open).is_none());
    appear.observe(None, None, open);
    assert_eq!(appear.leaving(open), Some((Page::Devices, 1.)));
    assert!(appear.moving(open));
    assert!(appear.leaving(open + LEAVE).is_none());
    assert!(!appear.moving(open + LEAVE));
    // Opening anything again ends it at once.
    appear.observe(None, None, open);
    appear.observe(Some(anchor), Some(Page::Devices), open);
    assert!(appear.leaving(open).is_none());
}

#[test]
fn a_closed_page_lets_go_of_what_it_read_once_it_has_gone() {
    let start = Instant::now();
    let mut appear = live();
    let anchor = Shown::Popover(point(px(200.), px(40.)));
    appear.observe(Some(anchor), Some(Page::Git), start);
    appear.keep(Page::Git, Kept::default());
    appear.observe(None, None, start);
    appear.settle(start);
    assert!(appear.kept.is_some(), "kept while it goes");
    appear.settle(start + LEAVE);
    assert!(appear.leaving.is_none());
    assert!(appear.kept.is_none(), "dropped once it has gone");
}

#[test]
fn other_menus_do_not_linger_after_closing() {
    let start = Instant::now();
    let mut appear = live();
    let anchor = Shown::Popover(point(px(10.), px(600.)));
    appear.observe(Some(anchor), Some(Page::Menu), start);
    appear.observe(None, None, start + ENTER);
    assert!(appear.leaving(start + ENTER).is_none());
}

#[gpui::test]
fn all_devices_draws_lower_while_it_rises(cx: &mut gpui::TestAppContext) {
    use crate::sidebar::layout_tests::fixture_window;
    let (view, cx) = cx.add_window_view(fixture_window);
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    // The footer sits at the window's foot, so the menu opens above it.
    let foot = cx.update(|window, _| window.viewport_size().height) - px(40.);
    let open = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, _| {
            view.menu.anchor = point(px(20.), foot);
            view.menu.page = Some(Page::Devices);
        });
    };
    open(cx);
    draw(cx);
    let settled = cx.debug_bounds("menu-panel").expect("All Devices draws");
    view.update(cx, |view, _| view.menu.page = None);
    draw(cx);
    view.update(cx, |view, _| view.menu.appear.live = true);
    open(cx);
    draw(cx);
    // Held at its start, however long the frames take: a start still ahead
    // reads as not begun.
    view.update(cx, |view, _| {
        view.menu.appear.since = Some(motion::now() + std::time::Duration::from_secs(3600));
    });
    draw(cx);
    let rising = cx.debug_bounds("menu-panel").expect("All Devices draws");
    assert!(
        rising.origin.y > settled.origin.y + px(1.),
        "{rising:?} rises to {settled:?}"
    );
}

#[gpui::test]
fn the_git_popover_is_drawn_as_it_goes_and_leaves_the_menu_reset(cx: &mut gpui::TestAppContext) {
    use crate::sidebar::layout_tests::fixture_window;
    let (view, cx) = cx.add_window_view(fixture_window);
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    view.update(cx, |view, _| {
        view.menu.appear.live = true;
        view.menu.anchor = point(px(200.), px(40.));
        view.menu.page = Some(Page::Git);
        view.menu.error = Some("push failed".into());
    });
    draw(cx);
    assert!(cx.debug_bounds("menu-leaving-shield").is_none());
    cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.menu.error, None);
        // Held at the start of its way out, however long the frames take.
        // Closing may already have drawn a frame that started it, so the
        // start is set as well as observed.
        let later = motion::now() + std::time::Duration::from_secs(3600);
        view.menu.appear.observe(None, None, later);
        view.menu.appear.leaving = Some((Page::Git, later));
    });
    draw(cx);
    // The copy going away was drawn, shielded, and the menu's own state is
    // as the reset left it once it was.
    assert!(cx.debug_bounds("menu-leaving-shield").is_some());
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.menu.error, None);
        assert!(view.menu.appear.leaving(motion::now()).is_some());
    });
}

#[gpui::test]
fn the_dimming_lifts_behind_the_picker_a_dialog_returns_to(cx: &mut gpui::TestAppContext) {
    use crate::sidebar::layout_tests::fixture_window;
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        let start = Instant::now();
        view.menu.appear.live = true;
        view.menu
            .appear
            .observe(Some(Shown::Dialog), Some(Page::Sessions), start);
        let open = start + ENTER;
        // Escape in the session editor goes back to the picker, a popover.
        view.menu.page = Some(Page::Sessions);
        view.menu.appear.observe(
            Some(Shown::Popover(point(px(10.), px(600.)))),
            Some(Page::Sessions),
            open,
        );
        assert!(!view.menu_dims());
        assert_eq!(view.menu.appear.backdrop(open), 1.);
        assert!(view.menu.appear.moving(open));
        assert_eq!(view.menu.appear.backdrop(open + LEAVE), 0.);
    });
}

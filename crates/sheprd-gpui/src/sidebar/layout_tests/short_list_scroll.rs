use super::*;
use gpui::{ScrollDelta, ScrollWheelEvent, VisualTestContext};

/// A window whose two hosts list one workspace each, far shorter than the
/// spaces list, drawn until it is settled at the top.
fn open(cx: &mut gpui::TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot(1)));
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.snapshot = Some(Arc::new(snapshot(1)));
        view.endpoints.push(remote);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
    (view, cx)
}

#[gpui::test]
fn the_wheel_over_a_list_that_fits_moves_and_pins_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.sidebar_scroll[0].max_offset().y,
            px(0.),
            "the list must fit"
        );
    });
    let list = cx.debug_bounds("spaces-scroll").unwrap();
    let local = cx.debug_bounds("host-local").unwrap();
    let remote = cx.debug_bounds("host-ssh:test").unwrap();
    // Several wheel steps, each drawn as the event loop would after it.
    for _ in 0..3 {
        cx.simulate_event(ScrollWheelEvent {
            position: list.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for selector in ["sticky-host-local", "sticky-host-ssh:test"] {
            assert!(cx.debug_bounds(selector).is_none(), "{selector} pinned");
        }
        assert_eq!(cx.debug_bounds("host-local").unwrap(), local);
        assert_eq!(cx.debug_bounds("host-ssh:test").unwrap(), remote);
        view.read_with(cx, |view, _| {
            assert_eq!(view.sidebar_scroll[0].offset().y, px(0.));
        });
    }
}

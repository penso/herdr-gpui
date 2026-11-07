//! The Ports icon's count: the user's own servers, each process once,
//! leaving out the system's; nothing while unknown or none.
use super::*;
use crate::{
    SheprdWindow,
    sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw},
};
use gpui::{AppContext as _, Entity, VisualTestContext, px, size};

fn system_listener(number: u16, pid: u32) -> Listener {
    Listener {
        name: "ControlCenter".into(),
        process: "ControlCenter".into(),
        system: true,
        ..listener(number, pid, 1)
    }
}

#[test]
fn the_count_is_each_users_server_once_without_the_systems() {
    let mut ports = LocalPorts::default();
    assert_eq!(ports.serving(), None);
    ports.seed(vec![
        listener(3000, 4242, 10),
        // The same process on a second port is one server.
        listener(3001, 4242, 10),
        listener(8080, 4343, 20),
        system_listener(5000, 100),
        system_listener(7000, 100),
    ]);
    assert_eq!(ports.serving(), Some(2));
    ports.seed(vec![system_listener(5000, 100)]);
    assert_eq!(ports.serving(), Some(0));
}

/// The count keeps a slower scan alive while the section is closed, and
/// opening the section asks for a listing at once.
#[cfg(unix)]
#[test]
fn counting_keeps_a_background_scan_that_opening_hurries() {
    let (mut ports, _updates, commands) = LocalPorts::fake();
    ports.counted = true;
    let now = Instant::now();
    ports.poll(false, now);
    assert!(ports.running());
    assert!(commands.try_recv().is_err());
    ports.poll(true, now);
    assert_eq!(commands.try_recv().unwrap(), Command::Refresh);
    ports.counted = false;
    ports.poll(false, now);
    assert!(!ports.running());
}

fn fixture(
    cx: &mut gpui::TestAppContext,
    counted: bool,
) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.config.sidebar_style.ports_count = counted;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

fn show(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, listing: Option<Vec<Listener>>) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.local_ports = LocalPorts::default();
            if let Some(listing) = listing {
                view.local_ports.seed(listing);
            }
            cx.notify();
        });
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

#[gpui::test]
fn the_ports_icon_shows_the_count_and_hides_it_at_zero(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, true);
    show(&view, cx, None);
    assert!(cx.debug_bounds("activity-ports").is_some());
    assert!(cx.debug_bounds("activity-ports-bubble").is_none());
    show(&view, cx, Some(vec![system_listener(5000, 100)]));
    assert!(cx.debug_bounds("activity-ports-bubble").is_none());
    show(
        &view,
        cx,
        Some(vec![listener(3000, 4242, 10), system_listener(5000, 100)]),
    );
    let bubble = cx.debug_bounds("activity-ports-bubble").unwrap();
    let bar = cx.debug_bounds("activity-bar").unwrap();
    assert!(bubble.right() <= bar.right(), "{bubble:?} {bar:?}");
    // A server stopping updates it.
    show(&view, cx, Some(Vec::new()));
    assert!(cx.debug_bounds("activity-ports-bubble").is_none());
}

#[gpui::test]
fn turning_the_count_off_hides_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, false);
    show(&view, cx, Some(vec![listener(3000, 4242, 10)]));
    assert!(cx.debug_bounds("activity-ports").is_some());
    assert!(cx.debug_bounds("activity-ports-bubble").is_none());
}

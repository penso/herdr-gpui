//! `[sidebar] automations = false`: the bar leaves the section off, and
//! nothing reads or starts a host's daemon.
use super::section::{draw, window};
use super::*;

#[gpui::test]
fn turning_automations_off_hides_the_section_and_stops_reading(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.automations.install_for_test(
                wanted(Machine::Local, "This computer"),
                Read::Ready(Arc::new(status())),
            );
            view.show_activity_panel(ActivityPanel::Automations, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("automations-section").is_some());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar_style.automations = false;
            view.poll_automations(cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("activity-automations").is_none());
    assert!(cx.debug_bounds("automations-section").is_none());
    assert!(cx.debug_bounds("activity-workspaces").is_some());
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.automations.hosts.is_empty(), "no host is read");
        assert_eq!(view.activity_section(), None, "back on Workspaces");
    });

    // Asking for it again is ignored while it is off.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Automations, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("automations-section").is_none());

    // Back on, the window returns to the section it was left on.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar_style.automations = true;
            view.poll_automations(cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("activity-automations").is_some());
    assert!(cx.debug_bounds("automations-section").is_some());
    cx.update(|_, cx| assert!(!view.read(cx).automations.hosts.is_empty()));
}

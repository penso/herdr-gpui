//! Back returns to the section that opened a pane before walking the pane
//! trail.
use super::{
    sections::{local, show},
    *,
};
use crate::endpoint::Step;

fn panel(view: &gpui::Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext) -> ActivityPanel {
    cx.update(|_, cx| view.read(cx).activity.panel)
}

#[gpui::test]
fn back_returns_to_the_section_that_opened_a_pane(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    show(&view, cx, ActivityPanel::Issues);
    // The section's view opens an agent's pane, leaving for Workspaces.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = view.activity.section_view.unwrap();
            view.leave_activity_section(id, cx);
        })
    });
    assert_eq!(panel(&view, cx), ActivityPanel::Workspaces);
    assert!(cx.update(|_, cx| view.read(cx).can_travel(Step::Back)));
    assert!(!cx.update(|_, cx| view.read(cx).can_travel(Step::Forward)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.travel(Step::Back, window, cx)));
    assert_eq!(panel(&view, cx), ActivityPanel::Issues);
    // Back there once; the next press walks the pane trail, which is empty.
    assert!(!cx.update(|_, cx| view.read(cx).can_travel(Step::Back)));
}

#[gpui::test]
fn picking_a_panel_forgets_the_section_to_return_to(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    show(&view, cx, ActivityPanel::PullRequests);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = view.activity.section_view.unwrap();
            view.leave_activity_section(id, cx);
        })
    });
    show(&view, cx, ActivityPanel::Workspaces);
    assert!(!cx.update(|_, cx| view.read(cx).can_travel(Step::Back)));
}

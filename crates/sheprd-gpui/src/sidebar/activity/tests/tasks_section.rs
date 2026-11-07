//! The bar offers Tasks while a project has Beads, on the shared view's own
//! list.
use super::{
    sections::{local, shown_list},
    *,
};
use crate::orchestrator::List;

#[gpui::test]
fn tasks_shows_only_once_a_project_has_beads(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.activity_panel_listed(ActivityPanel::Tasks));
            view.show_activity_panel(ActivityPanel::Tasks, window, cx);
            assert_ne!(view.activity_section(), Some(ActivityPanel::Tasks));

            view.activity.tasks_listed = true;
            view.show_activity_panel(ActivityPanel::Tasks, window, cx);
            assert_eq!(view.activity_section(), Some(ActivityPanel::Tasks));
        })
    });
    assert_eq!(shown_list(&view, cx), List::Tasks);
    // Once none has, the window goes back to Workspaces, keeping the pick.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.activity.tasks_listed = false;
            assert_eq!(view.activity_section(), None);
        })
    });
}

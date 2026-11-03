//! The Pull requests and Issues sections show one orchestrator view of every
//! repository the connected hosts' workspaces hold.
use super::*;
use crate::{
    orchestrator::{List, ViewId},
    sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw, snapshot},
    state::ConnectionStatus,
};
use gpui::{AppContext as _, Entity, VisualTestContext, px, size};
use sheprd_client::ConnectTarget;
use std::sync::Arc;

pub(super) fn local(
    cx: &mut gpui::TestAppContext,
) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            // The local session can run the orchestrator's scripts.
            view.endpoints[0].connection.target = ConnectTarget::Local;
            // w3 to w5 are agent-launcher's worktrees; the others share a folder.
            let mut listing = snapshot(4);
            listing.focused_workspace_id = Some("w0".into());
            view.live.snapshot = Some(Arc::new(listing));
            view.live.status = ConnectionStatus::Connected;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

pub(super) fn show(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, panel: ActivityPanel) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.show_activity_panel(panel, window, cx));
        full_draw(window, cx).clear(cx);
    });
}

fn views(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| view.read(cx).orchestrators.len())
}

fn shown_list(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> List {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let id = view.activity.section_view.unwrap();
        view.orchestrators[&id].view().read(cx).list()
    })
}

fn tick(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_orchestrators(window, cx)));
}

#[gpui::test]
fn a_section_makes_its_view_once_and_picks_its_list(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    assert_eq!(views(&view, cx), 0);
    show(&view, cx, ActivityPanel::Issues);
    assert_eq!(views(&view, cx), 1);
    assert_eq!(shown_list(&view, cx), List::Issues);
    // The view fills the section, holding the keyboard.
    assert!(cx.debug_bounds("activity-section").is_some());
    assert!(cx.debug_bounds("activity-section-empty").is_none());
    cx.update(|window, cx| {
        let view = view.read(cx);
        let id = view.activity.section_view.unwrap();
        let focus = view.orchestrators[&id]
            .view()
            .read(cx)
            .focus_handle()
            .clone();
        assert!(focus.contains_focused(window, cx));
    });
    // The other section reuses the view, on its own list.
    show(&view, cx, ActivityPanel::PullRequests);
    assert_eq!(views(&view, cx), 1);
    assert_eq!(shown_list(&view, cx), List::PullRequests);
    // The tick keeps the section where it is.
    tick(&view, cx);
    assert_eq!(views(&view, cx), 1);
}

#[gpui::test]
fn a_section_lists_every_repository_once_and_drops_closed_ones(cx: &mut gpui::TestAppContext) {
    let repositories = tempfile::tempdir().unwrap();
    for folder in ["worktrees", "other"] {
        gix::init(repositories.path().join(folder)).unwrap();
    }
    let (view, cx) = local(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.prepare_orchestrator_projects_for_tests(repositories.path(), cx);
        });
    });
    show(&view, cx, ActivityPanel::Issues);
    let keys = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let view = view.read(cx);
            let id = view.activity.section_view.unwrap();
            view.orchestrators[&id].view().read(cx).repo_keys()
        })
    };
    // Worktrees of one repository are one.
    assert_eq!(keys(cx).len(), 2);
    // Another workspace focused lists the same: the section is not the
    // focused workspace's.
    let first = cx.update(|_, cx| view.read(cx).activity.section_view);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if let Some(snapshot) = view.live.snapshot.as_mut() {
                Arc::make_mut(snapshot).focused_workspace_id = Some("w1".into());
            }
        })
    });
    tick(&view, cx);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity.section_view),
        first
    );
    assert_eq!(keys(cx).len(), 2);
    // Closing agent-launcher's worktree drops its repository.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if let Some(snapshot) = view.live.snapshot.as_mut() {
                Arc::make_mut(snapshot)
                    .workspaces
                    .retain(|workspace| workspace.worktree.is_none());
            }
        })
    });
    tick(&view, cx);
    assert_eq!(keys(cx).len(), 1);
}

#[gpui::test]
fn only_the_sections_view_opening_something_leaves_the_section(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    show(&view, cx, ActivityPanel::Issues);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = view.activity.section_view.unwrap();
            // Another view's run opening leaves this section alone.
            view.leave_activity_section(ViewId::test(u64::MAX), cx);
            assert_eq!(view.activity.panel, ActivityPanel::Issues);
            // The section's own, once it showed a pane, gives way to it.
            view.leave_activity_section(id, cx);
            assert_eq!(view.activity.panel, ActivityPanel::Workspaces);
            assert!(view.activity_section().is_none());
        })
    });
}

#[gpui::test]
fn a_link_leaves_the_section_only_for_a_page_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    show(&view, cx, ActivityPanel::Issues);
    let mut event = |url: &str| {
        let id = view.read_with(cx, |view, _| view.activity.section_view.unwrap());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.orchestrator_events
                    .push((id, crate::orchestrator::Event::OpenUrl(url.into())));
                view.poll_orchestrators(window, cx);
                view.activity.panel
            })
        })
    };
    // An address that is not a page opens nothing, so the list stays.
    assert_eq!(event("not a page"), ActivityPanel::Issues);
    // A page tab shows in Workspaces; the system browser, where builds have
    // no page tabs, leaves the list in place.
    let expected = if crate::browser::EMBEDDED {
        ActivityPanel::Workspaces
    } else {
        ActivityPanel::Issues
    };
    assert_eq!(event("https://example.com/issues/1"), expected);
}

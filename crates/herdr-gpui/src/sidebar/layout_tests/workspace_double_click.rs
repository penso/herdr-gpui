use super::*;
use crate::{
    NavigationTarget,
    menu::{Page, WorkspaceAction, workspace_tests::target_id},
};
use gpui::{MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, VisualTestContext};

fn connected(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    crate::bind_keys(cx);
    let mut view = fixture_window(window, cx);
    view.live.status = crate::state::ConnectionStatus::Connected;
    view
}

/// Both presses of a double-click at `position`, drawing after each as the
/// platform does, so the second lands on the row the first selected.
fn double_click(position: Point<Pixels>, cx: &mut VisualTestContext) {
    for click_count in 1..=2 {
        cx.simulate_event(MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        cx.simulate_event(MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
}

/// The row named `selector`, after a frame that lays it out.
fn row(selector: &'static str, cx: &mut VisualTestContext) -> Point<Pixels> {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.debug_bounds(selector).unwrap().center()
}

fn pending(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) -> Option<NavigationTarget<String>> {
    view.read_with(cx, |view, _| view.pending_navigation.clone())
}

/// A single click selects at once and opens nothing, so selecting is never
/// held back waiting to see whether a second click follows.
#[gpui::test]
fn a_single_click_only_selects_the_workspace(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(672.)));
    let position = row("row-agent-launcher", cx);

    cx.simulate_click(position, Modifiers::default());
    assert_eq!(
        pending(&view, cx),
        Some(NavigationTarget::Workspace("w3".into()))
    );
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
    });
}

/// The second click of a double-click opens the dialog the row menu's Rename
/// opens, holding the workspace's name and taking the keys; Escape cancels it.
#[gpui::test]
fn double_clicking_a_workspace_opens_its_rename_dialog(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(672.)));
    let position = row("row-agent-launcher", cx);

    double_click(position, cx);
    assert_eq!(
        pending(&view, cx),
        Some(NavigationTarget::Workspace("w3".into()))
    );
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.menu.page, Some(Page::Dialog(WorkspaceAction::Rename)));
        assert_eq!(target_id(view), Some("w3"));
        let label = &view.live.snapshot.as_ref().unwrap().workspaces[3].label;
        assert_eq!(
            view.menu.input.as_ref().map(|input| &input.text),
            Some(label)
        );
        assert!(view.menu.focus.is_focused(window));
    });
    assert!(cx.debug_bounds("menu-overlay").is_some());

    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(view.menu.input.is_none());
    });
}

/// A linked worktree's menu offers Rename too, so its row renames itself,
/// not the main checkout it hangs from.
#[gpui::test]
fn double_clicking_a_worktree_renames_that_worktree(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(672.)));
    let position = row("row-sidebar-child", cx);

    double_click(position, cx);
    assert_eq!(
        pending(&view, cx),
        Some(NavigationTarget::Workspace("w4".into()))
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::Dialog(WorkspaceAction::Rename)));
        assert_eq!(target_id(view), Some("w4"));
        let label = &view.live.snapshot.as_ref().unwrap().workspaces[4].label;
        assert_eq!(
            view.menu.input.as_ref().map(|input| &input.text),
            Some(label)
        );
    });
}

/// The fold chevron keeps its clicks from the row, so folding and unfolding
/// a group fast neither selects nor renames it.
#[gpui::test]
fn double_clicking_the_fold_chevron_does_not_rename(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(672.)));
    let position = row("collapse-3", cx);

    double_click(position, cx);
    assert_eq!(pending(&view, cx), None);
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        // Folded by the first click and unfolded by the second.
        assert!(!view.collapsed_repos.contains(REPO_KEY));
    });
    assert!(cx.debug_bounds("row-sidebar-child").is_some());
}

/// Without a connection the row's menu cannot open, so neither can Rename.
#[gpui::test]
fn double_clicking_while_disconnected_opens_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = connected(window, cx);
        view.live.status = crate::state::ConnectionStatus::Disconnected;
        view
    });
    cx.simulate_resize(size(px(800.), px(672.)));
    let position = row("row-agent-launcher", cx);

    double_click(position, cx);
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
    });
}

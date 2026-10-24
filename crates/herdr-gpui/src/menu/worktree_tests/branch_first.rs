use super::*;

fn branch(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone())
}

fn naming(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| view.read(cx).worktree_name_focused(window, cx))
}

/// The dialog opens on the branch, its proposal selected so typing replaces
/// it, and the name stays empty for the daemon to fill in.
#[gpui::test]
fn the_dialog_opens_on_the_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    assert!(!naming(&view, cx));
    assert!(!cx.update(|window, cx| view.read(cx).worktree_search_focused(window, cx)));
    cx.update(|_, cx| {
        let input = view.read(cx).menu.input.as_ref().unwrap();
        assert!(input.text.starts_with("worktree/"), "{}", input.text);
        assert_eq!(input.selection, 0..input.text.len());
    });

    cx.simulate_input("feature/login");
    assert_eq!(branch(&view, cx), "feature/login");
    assert_eq!(cx.update(|_, cx| view.read(cx).worktree_name(cx)), None);

    // Enter submits from the branch; the fixture has no daemon.
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.menu.page,
            Some(Page::Dialog(WorkspaceAction::NewWorktree))
        );
        assert!(view.menu.error.is_some());
    });
}

/// Stepping round the strip with Ctrl-Tab back to the form puts typing on
/// the branch again, even when the name had it before.
#[gpui::test]
fn returning_to_the_form_tab_focuses_the_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    click_name(cx);
    assert!(naming(&view, cx));
    // Without GitHub the strip is new, existing and branch.
    cx.simulate_keystrokes("ctrl-tab ctrl-tab ctrl-tab");
    assert_eq!(tab(&view, cx), Tab::New);
    assert!(!naming(&view, cx));
    assert!(!cx.update(|window, cx| view.read(cx).worktree_search_focused(window, cx)));
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("fix/typo");
    assert_eq!(branch(&view, cx), "fix/typo");
    assert_eq!(cx.update(|_, cx| view.read(cx).worktree_name(cx)), None);
}

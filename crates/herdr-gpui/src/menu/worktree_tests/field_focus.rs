use super::*;

/// The branch form's field that has typing.
fn focused(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> &'static str {
    cx.update(|window, cx| {
        let view = view.read(cx);
        if view.worktree_search_focused(window, cx) {
            "search"
        } else if view.worktree_name_focused(window, cx) {
            "name"
        } else if view.menu.focus.is_focused(window) {
            "branch"
        } else {
            "none"
        }
    })
}

fn branch(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone())
}

fn name(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        let source = view.read(cx).menu.worktree.as_ref().unwrap();
        source.name.read(cx).text().to_owned()
    })
}

/// Tab and Shift-Tab move typing through the form's fields in the order they
/// are drawn (search, name, branch), wrapping at either end, and never leave
/// the tab. The field Tab lands in has its text selected, so typing replaces
/// it, as in a native form.
#[gpui::test]
fn tab_moves_between_the_forms_fields(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    assert_eq!(focused(&view, cx), "branch");

    // The issue's case: from the branch back to the name above it, and on.
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(focused(&view, cx), "name");
    cx.simulate_input("Batch Jobs UI");
    assert_eq!(branch(&view, cx), "worktree/batch-jobs-ui");
    cx.simulate_keystrokes("tab");
    assert_eq!(focused(&view, cx), "branch");
    cx.update(|_, cx| {
        let input = view.read(cx).menu.input.as_ref().unwrap();
        assert_eq!(input.selection, 0..input.text.len());
    });

    // Past the branch, Tab wraps to the search above the tabs, then the name.
    cx.simulate_keystrokes("tab");
    assert_eq!(focused(&view, cx), "search");
    cx.simulate_keystrokes("tab");
    assert_eq!(focused(&view, cx), "name");
    cx.simulate_input("Login");
    assert_eq!(name(&view, cx), "Login");
    cx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(focused(&view, cx), "branch");
    assert_eq!(tab(&view, cx), Tab::New);

    // Typing over the selected branch makes it the user's.
    cx.simulate_input("feat/login page");
    assert_eq!(branch(&view, cx), "feat/login-page");
    assert_eq!(name(&view, cx), "Login");
}

/// Ctrl-Tab and Ctrl-Shift-Tab step through the strip from any field, as they
/// step through the palette's filters and the add device rail, and the strip
/// names its key.
#[gpui::test]
fn ctrl_tab_steps_through_the_tabs(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    let strip = cx.debug_bounds("worktree-tabs").unwrap();
    let hint = cx.debug_bounds("worktree-tabs-hint").unwrap();
    let last = cx.debug_bounds("worktree-tab-issues").unwrap();
    assert!(last.right() < hint.left() && hint.right() <= strip.right());

    click_name(cx);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(tab(&view, cx), Tab::Existing);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(tab(&view, cx), Tab::Branches);
    cx.simulate_keystrokes("ctrl-shift-tab ctrl-shift-tab");
    assert_eq!(tab(&view, cx), Tab::New);
    assert_eq!(focused(&view, cx), "branch");
    // Other modifiers belong to the app and the system.
    for keys in ["alt-tab", "cmd-tab"] {
        cx.simulate_keystrokes(keys);
        assert_eq!(tab(&view, cx), Tab::New, "{keys}");
    }
}

/// A listing has a single field, so Tab moves through its rows as it moves
/// through the palette's, and the tab stays put.
#[gpui::test]
fn tab_moves_through_a_listings_rows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    cx.simulate_keystrokes("ctrl-tab ctrl-tab ctrl-tab");
    draw(cx);
    assert_eq!(tab(&view, cx), Tab::Items(Kind::PullRequest));
    let selected = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).menu.worktree.as_ref().unwrap().selected)
    };
    assert_eq!(selected(cx), 0);
    cx.simulate_keystrokes("tab");
    assert_eq!(selected(cx), 1);
    cx.simulate_keystrokes("tab");
    assert_eq!(selected(cx), 0);
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(selected(cx), 1);
    assert_eq!(tab(&view, cx), Tab::Items(Kind::PullRequest));
    assert_eq!(focused(&view, cx), "search");
}

use super::*;

fn branch(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone())
}

/// Move typing to the branch field, as a click on it does.
fn click_branch(cx: &mut VisualTestContext) {
    let field = cx.debug_bounds("dialog-input").unwrap();
    cx.simulate_click(field.center(), gpui::Modifiers::none());
}

/// Back on the name, more typing and clearing it leave `edited` alone.
fn name_no_longer_reaches(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, edited: &str) {
    click_name(cx);
    assert!(cx.update(|window, cx| view.read(cx).worktree_name_focused(window, cx)));
    cx.simulate_input(" signup");
    assert_eq!(branch(view, cx), edited);
    cx.simulate_keystrokes("cmd-a backspace");
    assert_eq!(branch(view, cx), edited);
}

/// The proposed branch is derived from the name as it is typed, and the
/// first proposal comes back whenever the name has nothing to derive from.
#[gpui::test]
fn the_proposed_branch_follows_the_typed_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    let proposal = branch(&view, cx);
    assert!(proposal.starts_with("worktree/"), "{proposal}");

    click_name(cx);
    cx.simulate_input("Fix login");
    assert_eq!(branch(&view, cx), "worktree/fix-login");
    // Selected like the first proposal, so typing in the field replaces it.
    cx.update(|_, cx| {
        let input = view.read(cx).menu.input.as_ref().unwrap();
        assert_eq!(input.selection, 0..input.text.len());
    });

    // Nothing the daemon's slug keeps: the first proposal, not an empty branch.
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("\u{65e5}\u{672c}\u{8a9e}");
    assert_eq!(branch(&view, cx), proposal);
    cx.simulate_input(" v2");
    assert_eq!(branch(&view, cx), "worktree/v2");

    cx.simulate_keystrokes("cmd-a backspace");
    assert_eq!(cx.update(|_, cx| view.read(cx).worktree_name(cx)), None);
    assert_eq!(branch(&view, cx), proposal);
}

/// A shorter proposal is painted from its start, not at the scroll offset a
/// longer one needed: a click left of its text lands before its first byte.
#[gpui::test]
fn a_shorter_proposal_is_not_scrolled_out_of_view(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(640.), gpui::px(400.)));
    open_dialog(&view, cx, false, Tab::New);
    // The longest proposal there is, wider than the field at this size.
    click_name(cx);
    cx.simulate_input(&"word ".repeat(20));
    draw(cx);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Fix");
    draw(cx);
    assert_eq!(branch(&view, cx), "worktree/fix");

    let field = cx.debug_bounds("dialog-input").unwrap();
    cx.simulate_click(
        gpui::point(field.left() + gpui::px(1.), field.center().y),
        gpui::Modifiers::none(),
    );
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.input.as_ref().unwrap().selection, 0..0));
}

/// Replacing the first proposal makes the branch the user's.
#[gpui::test]
fn a_replaced_proposal_stops_following_the_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    click_branch(cx);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("feature/login");
    assert_eq!(branch(&view, cx), "feature/login");
    name_no_longer_reaches(&view, cx, "feature/login");
}

/// Editing a branch derived from the name makes it the user's too.
#[gpui::test]
fn an_edited_derived_branch_stops_following_the_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    click_name(cx);
    cx.simulate_input("Fix");
    click_branch(cx);
    cx.simulate_keystrokes("end");
    cx.simulate_input("-2");
    assert_eq!(branch(&view, cx), "worktree/fix-2");
    name_no_longer_reaches(&view, cx, "worktree/fix-2");
}

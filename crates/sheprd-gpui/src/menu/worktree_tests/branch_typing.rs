use super::*;

fn branch(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().text.clone())
}

fn selection(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> std::ops::Range<usize> {
    cx.update(|_, cx| view.read(cx).menu.input.as_ref().unwrap().selection.clone())
}

/// The issue's case: a branch typed with spaces reads as a valid one as it is
/// typed, keeping its case and namespace, so submitting it is never refused
/// for its characters.
#[gpui::test]
fn the_branch_is_formatted_as_it_is_typed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    cx.simulate_input("matt/Batch Jobs  table layout");
    assert_eq!(branch(&view, cx), "matt/Batch-Jobs-table-layout");
    // The caret stays after what was typed, so typing simply goes on.
    let end = branch(&view, cx).len();
    assert_eq!(selection(&view, cx), end..end);
    cx.simulate_input("..v2");
    assert_eq!(branch(&view, cx), "matt/Batch-Jobs-table-layout.v2");
    assert!(crate::worktree::validate_branch(&branch(&view, cx)).is_ok());

    // Between two characters, a key that would form a refused sequence types
    // nothing, and the text around the caret is left as it was.
    cx.simulate_keystrokes("home");
    cx.simulate_input("-");
    assert_eq!(branch(&view, cx), "matt/Batch-Jobs-table-layout.v2");
    assert_eq!(selection(&view, cx), 0..0);
    cx.simulate_keystrokes("right right right right right");
    cx.simulate_input("/");
    assert_eq!(branch(&view, cx), "matt/Batch-Jobs-table-layout.v2");
    cx.simulate_input("x");
    assert_eq!(branch(&view, cx), "matt/xBatch-Jobs-table-layout.v2");
    assert_eq!(selection(&view, cx), 6..6);
}

/// Greptile's case: deleting what parts two dots, with Backspace over a
/// selection or before it, Delete, or a cut, leaves one dot rather than
/// `a..b`, with the caret where the deletion left it.
#[gpui::test]
fn deleting_between_two_dots_leaves_one(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    for keys in [
        "left left shift-left backspace",
        "left left backspace",
        "left left left delete",
        "left left shift-left cmd-x",
    ] {
        cx.simulate_keystrokes("end shift-home");
        cx.simulate_input("a.x.b");
        assert_eq!(branch(&view, cx), "a.x.b", "{keys}");
        cx.simulate_keystrokes(keys);
        assert_eq!(branch(&view, cx), "a.b", "{keys}");
        assert_eq!(selection(&view, cx), 2..2, "{keys}");
        assert!(crate::worktree::git_accepts(&branch(&view, cx)), "{keys}");
    }
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("x".into())
    );

    // Deleting the start leaves no dash to start the name.
    cx.simulate_keystrokes("end shift-home");
    cx.simulate_input("a-b");
    cx.simulate_keystrokes("home shift-right backspace");
    assert_eq!(branch(&view, cx), "b");
    assert_eq!(selection(&view, cx), 0..0);
    assert!(crate::worktree::git_accepts(&branch(&view, cx)));
}

/// Typing over a selection replaces it only with what the field keeps, so a
/// key that would join a refused sequence leaves the selection as it was.
#[gpui::test]
fn typing_over_a_selection_keeps_the_branch_valid(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    cx.simulate_input("a.x.b");
    cx.simulate_keystrokes("left left shift-left");
    cx.simulate_input(".");
    assert_eq!(branch(&view, cx), "a.x.b");
    assert_eq!(selection(&view, cx), 2..3);
    cx.simulate_input(" ");
    assert_eq!(branch(&view, cx), "a.-.b");
    assert_eq!(selection(&view, cx), 3..3);
    assert!(crate::worktree::git_accepts(&branch(&view, cx)));
}

/// Pasted text is formatted the same way, without the newline a copied line
/// often ends in.
#[gpui::test]
fn a_pasted_branch_is_formatted(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "feat/Login v2: retry?\n".into(),
    ));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(branch(&view, cx), "feat/Login-v2-retry");
}

/// A key that formats to nothing leaves the selected proposal in place, and
/// since nothing was typed, the proposal still follows the name.
#[gpui::test]
fn a_refused_key_keeps_the_proposal_following_the_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    let proposal = branch(&view, cx);
    for key in [" ", "-", ".", "/"] {
        cx.simulate_input(key);
        assert_eq!(branch(&view, cx), proposal, "{key:?}");
        assert_eq!(selection(&view, cx), 0..proposal.len(), "{key:?}");
    }
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_input("Fix login");
    assert_eq!(branch(&view, cx), "worktree/fix-login");
}

/// Once the user types into the branch, formatted or not, it is theirs: the
/// name no longer reaches it.
#[gpui::test]
fn a_formatted_branch_stops_following_the_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_input("Batch Jobs UI");
    assert_eq!(branch(&view, cx), "worktree/batch-jobs-ui");
    cx.simulate_keystrokes("tab end");
    cx.simulate_input(" v2");
    assert_eq!(branch(&view, cx), "worktree/batch-jobs-ui-v2");
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_input("Something else");
    assert_eq!(branch(&view, cx), "worktree/batch-jobs-ui-v2");
}

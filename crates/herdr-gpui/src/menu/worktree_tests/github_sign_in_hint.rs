use super::*;
use crate::menu::worktree_source::signed_out_hint;

/// A faded GitHub tab says why on hover: Herdr keeps its own GitHub sign-in,
/// so a `gh` login elsewhere does not enable it.
#[gpui::test]
fn a_signed_out_github_tab_explains_how_to_enable_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    let hover = |cx: &mut VisualTestContext, selector: &'static str| {
        let tab = cx.debug_bounds(selector).unwrap();
        cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::none());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        draw(cx);
        cx.debug_bounds("hint")
    };
    for selector in ["worktree-tab-PR", "worktree-tab-issues"] {
        let hint = hover(cx, selector);
        assert!(hint.is_some(), "{selector} shows a sign-in hint");
    }
    // Tabs that work without an account need no explanation.
    let away = cx.debug_bounds("dialog-cancel").unwrap();
    cx.simulate_mouse_move(away.center(), None, gpui::Modifiers::none());
    draw(cx);
    assert!(hover(cx, "worktree-tab-branch").is_none());
}

/// With an account the GitHub tabs work, so they carry no sign-in hint.
#[gpui::test]
fn a_connected_github_tab_has_no_sign_in_hint(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, true, Tab::New);
    let tab = cx.debug_bounds("worktree-tab-issues").unwrap();
    cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    draw(cx);
    assert!(cx.debug_bounds("hint").is_none());
}

#[test]
fn the_hint_names_the_listing_and_where_to_sign_in() {
    for kind in [Kind::PullRequest, Kind::Issue] {
        let hint = signed_out_hint(kind);
        assert!(hint.contains("Sign in to GitHub in Herdr"), "{hint}");
        assert!(hint.contains("account button"), "{hint}");
    }
    assert!(signed_out_hint(Kind::PullRequest).ends_with("pull requests."));
    assert!(signed_out_hint(Kind::Issue).ends_with("issues."));
}

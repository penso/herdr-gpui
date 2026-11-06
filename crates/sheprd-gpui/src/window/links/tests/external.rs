use super::{file_handlers::hyperlinked, *};
use crate::menu::Page;

/// A window on a daemon without link methods, showing `notes.md` as an
/// explicit hyperlink to `target`.
pub(super) fn linked<'a>(
    target: &str,
    cx: &'a mut gpui::TestAppContext,
) -> (
    Entity<SheprdWindow>,
    &'a mut VisualTestContext,
    Point<Pixels>,
) {
    let peer = MockPeer::new();
    let (view, cx) = window(&peer, false, cx);
    view.update(cx, |view, _| view.live.surface = Some(hyperlinked(target)));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let link = at(&view, cx, 2, 0);
    (view, cx, link)
}

#[gpui::test]
fn any_other_scheme_opens_only_after_the_user_confirms_it(cx: &mut gpui::TestAppContext) {
    let (view, cx, link) = linked("ssh://example.com", cx);
    cx.simulate_click(link, Modifiers::secondary_key());
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::OpenLink))
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("open-link-url").is_some());

    // "No" opens nothing and closes the question.
    let no = cx.debug_bounds("open-link-no").unwrap().center();
    cx.simulate_click(no, Modifiers::default());
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));

    // "Yes" opens the address that was shown.
    cx.simulate_click(link, Modifiers::secondary_key());
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let yes = cx.debug_bounds("open-link-yes").unwrap().center();
    cx.simulate_click(yes, Modifiers::default());
    assert_eq!(cx.opened_url().as_deref(), Some("ssh://example.com"));
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
}

#[gpui::test]
fn the_question_answers_to_enter_and_escape(cx: &mut gpui::TestAppContext) {
    let (view, cx, link) = linked("ssh://example.com", cx);
    cx.simulate_click(link, Modifiers::secondary_key());
    cx.simulate_keystrokes("escape");
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    cx.simulate_click(link, Modifiers::secondary_key());
    cx.simulate_keystrokes("enter");
    assert_eq!(cx.opened_url().as_deref(), Some("ssh://example.com"));
}

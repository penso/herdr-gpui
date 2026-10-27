//! The GitLab page from the keyboard: Tab walks its controls in the order
//! they show, and Enter uses the selected one, as on the GitHub page.
use super::*;
use crate::menu::gitlab::keys::Action;

fn show(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, hosts: Vec<HostStatus>) {
    view.update(cx, |view, cx| {
        view.menu.gitlab.hosts = hosts;
        cx.notify();
    });
    draw(cx);
}

fn selected(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Action> {
    view.read_with(cx, |view, _| view.menu.gitlab.selected.clone())
}

#[gpui::test]
fn tab_walks_the_controls_and_enter_uses_one(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    show(
        &view,
        cx,
        vec![status(Some(Source::Saved), Some(Ok("Akanoa")))],
    );
    let actions = view.read_with(cx, |view, _| view.menu.gitlab.actions());
    assert_eq!(
        actions,
        [
            Action::GitHub,
            Action::Remove("gitlab.com".into()),
            Action::Paste,
            Action::Add,
            Action::Close,
        ]
    );
    cx.simulate_keystrokes("tab tab");
    assert_eq!(
        selected(&view, cx),
        Some(Action::Remove("gitlab.com".into()))
    );
    cx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(selected(&view, cx), Some(Action::Close));
    cx.simulate_keystrokes("tab");
    assert_eq!(selected(&view, cx), Some(Action::GitHub));
    // Enter opens the token form from its link.
    cx.simulate_keystrokes("tab tab enter");
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.menu.gitlab.form));
    // The form keeps its own keys: Escape goes back to the instances.
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(!view.menu.gitlab.form);
        assert_eq!(view.menu.page, Some(Page::GitLab));
    });
    // The link stays selected; back past Forget to the GitHub tab, which
    // switches pages.
    cx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(selected(&view, cx), Some(Action::GitHub));
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert_eq!(view.menu.page, Some(Page::GitHub)));
}

#[gpui::test]
fn the_welcome_page_reaches_connect_from_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    let welcome = HostStatus {
        can_connect: true,
        ..status(None, None)
    };
    show(&view, cx, vec![welcome]);
    let actions = view.read_with(cx, |view, _| view.menu.gitlab.actions());
    assert_eq!(
        actions,
        [
            Action::GitHub,
            Action::Connect("gitlab.com".into()),
            Action::Paste,
            Action::Close,
        ]
    );
    cx.simulate_keystrokes("tab tab");
    assert_eq!(
        selected(&view, cx),
        Some(Action::Connect("gitlab.com".into()))
    );
}

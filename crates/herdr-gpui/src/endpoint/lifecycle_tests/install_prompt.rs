use super::*;
use crate::{menu::Page, sidebar::layout_tests::full_draw};

/// Exercise the real retry path against the fixture's missing socket, without
/// discovering a personal daemon or waiting for the backoff clock.
fn retry(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    wait_until(|| {
        view.endpoints[0].poll(Instant::now());
        view.endpoints[0].connection.handle.is_none()
    });
    let generation = view.endpoints[0].generation;
    view.endpoints[0].retry_at = Instant::now();
    view.poll_endpoints(cx);
    assert!(view.endpoints[0].generation > generation);
    assert_eq!(view.selected_generation, view.endpoints[0].generation);
}

#[gpui::test]
fn install_prompt_survives_background_retries_and_remains_clickable(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update_in(cx, |view, window, cx| {
        // The startup warning is offered once, not reopened on each attempt.
        view.install_warning_shown = true;
        view.show_install_modal(window, cx);
        for _ in 0..3 {
            retry(view, cx);
            assert_eq!(view.menu.page, Some(Page::Install));
            assert!(view.menu.focus.is_focused(window));
            assert!(view.install_warning_shown);
        }
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let install = cx.debug_bounds("menu-install").unwrap();
    cx.simulate_click(install.center(), Default::default());
    assert_eq!(cx.opened_url().as_deref(), Some(crate::about::WEBSITE));
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::Install))
    });

    let dismiss = cx.debug_bounds("menu-dismiss").unwrap();
    cx.simulate_click(dismiss.center(), Default::default());
    view.update_in(cx, |view, window, cx| {
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
        retry(view, cx);
        assert!(view.menu.page.is_none(), "dismissal survives later retries");
        assert!(view.install_warning_shown);
    });
}

#[gpui::test]
fn install_prompt_does_not_change_explicit_reconnect_or_daemon_dialog_fencing(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update_in(cx, |view, window, cx| {
        view.install_warning_shown = true;
        view.show_install_modal(window, cx);
        view.reconnect(cx);
        assert!(view.menu.page.is_none());
        assert!(
            !view.install_warning_shown,
            "manual retry can offer the warning again"
        );

        view.open_menu(window, cx);
        view.menu.page = Some(Page::Workspace);
        assert!(view.menu_target_current());
        retry(view, cx);
        assert!(
            view.menu.page.is_none(),
            "daemon-bound dialogs must still close"
        );
        assert!(!view.menu_target_current());
    });
}

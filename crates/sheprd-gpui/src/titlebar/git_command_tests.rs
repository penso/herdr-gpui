#![allow(clippy::unwrap_used)]
use crate::{
    controls::Command,
    git::{Git, Status},
    menu::Page,
    pull_request::Input,
    sidebar::layout_tests::REPO_KEY,
};
use gpui::{TestAppContext, px, size};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
}

/// The `git_menu` binding opens the menu where clicking the titlebar's Git
/// button does, and does nothing without a tracked checkout.
#[gpui::test]
fn the_git_menu_command_opens_the_menu_at_the_git_button(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::titlebar::tests::header_window);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.run_until_parked();
    draw(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::GitMenu, window, cx)));
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, None));

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(
                Input {
                    checkout: None,
                    repo_key: Some(REPO_KEY.into()),
                    branch: "develop".into(),
                },
                Status::default(),
            );
            cx.notify();
        })
    });
    draw(cx);
    let button = cx.debug_bounds("titlebar-git").unwrap();
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::GitMenu, window, cx)));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.menu.page, Some(Page::Git));
        assert_eq!(view.menu.anchor, button.center());
    });
}

/// Zen mode hides the titlebar, so the menu opens from the window's top-right
/// corner rather than from where the hidden button last painted.
#[gpui::test]
fn the_git_menu_command_opens_from_the_corner_in_zen_mode(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::titlebar::tests::header_window);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.git = Git::fixture(
                Input {
                    checkout: None,
                    repo_key: Some(REPO_KEY.into()),
                    branch: "develop".into(),
                },
                Status::default(),
            );
            cx.notify();
        })
    });
    draw(cx);
    let button = cx.debug_bounds("titlebar-git").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ToggleZenMode, window, cx);
            view.command(Command::GitMenu, window, cx);
        })
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.menu.page, Some(Page::Git));
        assert_ne!(view.menu.anchor, button.center());
        assert_eq!(view.menu.anchor, gpui::point(px(1200. - 24.), px(24.)));
    });
}

/// The `review_changes` binding goes through the same path as the menu's
/// Review row, so without a checkout it says so instead of doing nothing.
#[gpui::test]
fn the_review_command_says_when_there_is_no_checkout(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::titlebar::tests::header_window);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ReviewChanges, window, cx)
        })
    });
    cx.update(|_, cx| {
        let flash = view
            .read(cx)
            .flash
            .as_ref()
            .map(|(flash, _)| flash.text.clone());
        assert_eq!(flash.as_deref(), Some("No local checkout to review"));
    });
}

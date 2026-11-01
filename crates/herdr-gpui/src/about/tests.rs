#![allow(clippy::unwrap_used)]
use super::*;
use core::prelude::v1::test;

mod en_xa;

/// The visible copyright must not drift from the repository's NOTICE.
#[test]
fn copyright_tracks_the_repository_notice() {
    let notice = include_str!("../../../../NOTICE");
    let holder = notice
        .lines()
        .find_map(|line| line.strip_prefix("Copyright "))
        .unwrap();
    let copyright = format!("{COPYRIGHT_YEAR} {AUTHOR} {LICENSE}");
    assert!(copyright.contains(holder), "{copyright} lacks {holder}");
    assert!(notice.contains("Apache License, Version 2.0"));
    assert!(LICENSE.contains("Apache-2.0"));
}

/// Classic macOS puts About first in the application menu, above a separator.
#[test]
fn about_leads_the_application_menu() {
    let menus = crate::menus(&crate::config::Config::default());
    let application = menus.first().unwrap();
    assert_eq!(application.name.as_ref(), "Herdr");
    let MenuItem::Action { name, action, .. } = application.items.first().unwrap() else {
        panic!("the application menu must start with an action");
    };
    assert_eq!(name.as_ref(), "About Herdr");
    assert!(action.partial_eq(&crate::RunCommand {
        command: crate::controls::Command::About,
    }));
    assert!(matches!(application.items[1], MenuItem::Separator));
    assert!(matches!(
        application.items.last().unwrap(),
        MenuItem::Action { name, .. } if name.as_ref() == "Quit Herdr"
    ));
}

#[gpui::test]
fn about_box_shows_build_identity_and_opens_links(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for width in [800., 360.] {
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_about(window, cx)));
        cx.simulate_resize(size(px(width), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let about = cx.debug_bounds("about").unwrap();
        let icon = cx.debug_bounds("about-icon").unwrap();
        let name = cx.debug_bounds("about-name").unwrap();
        let version = cx.debug_bounds("about-version").unwrap();
        let disclaimer = cx.debug_bounds("about-unaffiliated").unwrap();
        let copyright = cx.debug_bounds("about-copyright").unwrap();
        let close = cx.debug_bounds("about-close").unwrap();
        assert_eq!(icon.size, size(px(96.), px(96.)));
        for row in [icon, name, version, disclaimer, copyright, close] {
            assert!(row.top() >= about.top() && row.bottom() <= about.bottom());
            assert!(row.left() >= about.left() && row.right() <= about.right());
        }
        assert!(icon.bottom() <= name.top());
        assert!(name.bottom() <= version.top());
        assert!(version.bottom() <= disclaimer.top());
        assert!(disclaimer.bottom() <= copyright.top());
        assert!(copyright.bottom() <= close.top());
        assert!(about.right() <= px(width));
        assert_eq!(
            cx.debug_bounds("about-build").is_some(),
            env!("HERDR_BUILD_WORKTREE") == "1"
        );

        for (selector, url) in [
            ("about-homepage", HOMEPAGE.to_owned()),
            ("about-website", WEBSITE.to_owned()),
            ("about-repository", REPOSITORY.to_owned()),
            ("about-author", "https://pen.so".to_owned()),
            ("about-twitter", "https://x.com/fabienpenso".to_owned()),
        ] {
            let link = cx.debug_bounds(selector).unwrap();
            cx.simulate_click(link.center(), Modifiers::default());
            assert_eq!(cx.opened_url(), Some(url));
            view.read_with(cx, |view, _| {
                assert_eq!(view.menu.page, Some(Page::About));
            });
        }

        // Both the OK button and Escape close the box without other effects.
        cx.simulate_click(close.center(), Modifiers::default());
        view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_about(window, cx)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("escape");
        view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    }
}

#[gpui::test]
fn the_in_app_menu_opens_the_about_box(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // Anchor the popup to the status bar so the row is on screen.
            view.menu.anchor = point(px(60.), px(560.));
            view.open_menu(window, cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let row = cx.debug_bounds("menu-about").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    view.read_with(cx, |view, _| assert_eq!(view.menu.page, Some(Page::About)));
}

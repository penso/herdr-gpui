//! Scaled rows, shortcut search, Preferences, GitHub, theme, palette, and close modal checks.
use super::super::*;

pub(super) fn check_scaled_sidebar(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar.size = 24.;
            view.marked = "composition".into();
            view.open_keybinds(window, cx);
            assert!(view.marked.is_empty());
        });
        full_draw(window, cx).clear(cx);
        assert!(!view.read(cx).focus.is_focused(window));
    });
    let line_height = cx.update(|_, cx| crate::sidebar::line_height(&view.read(cx).config.sidebar));
    assert_eq!(
        cx.debug_bounds("row-herdr").unwrap().size.height,
        px(2. * line_height + 8.)
    );
    assert_eq!(
        cx.debug_bounds("name-herdr").unwrap().size.height,
        px(line_height)
    );
    let title = cx.debug_bounds("name-herdr").unwrap();
    let detail = cx.debug_bounds("detail-herdr").unwrap();
    let icon = cx.debug_bounds("github-herdr").unwrap();
    assert_eq!(title.bottom(), detail.top());
    assert_eq!(detail.size.height, px(line_height));
    assert_eq!(
        title.size.width,
        px(crate::sidebar::LABEL_WIDTH - crate::sidebar::ICON_RESERVE)
    );
    assert_eq!(title.right(), detail.right());
    assert_eq!(icon.center().y, title.center().y);
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
}

pub(super) fn check_shortcut_search(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes("cmd-/");
    let shortcut_search = cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let search = view.read(cx).menu.keybinds_search.as_ref().unwrap().clone();
        assert!(search.read(cx).focus.is_focused(window));
        search
    });
    cx.simulate_input("pane zoom");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(shortcut_search.read(cx).text(), "pane zoom");
    });
    assert!(cx.debug_bounds("shortcut-Toggle Pane Zoom").is_some());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("no-shortcut-matches-xyz");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("keybinds-empty").is_some());
    cx.simulate_keystrokes("cmd-w");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Keybinds)));
    cx.simulate_keystrokes("escape cmd-/");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let search = view.read(cx).menu.keybinds_search.as_ref().unwrap().clone();
        assert!(search.read(cx).text().is_empty());
        search.update(cx, |input, cx| {
            gpui::EntityInputHandler::replace_and_mark_text_in_range(
                input,
                None,
                "pane",
                Some(4..4),
                window,
                cx,
            )
        });
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Keybinds));
        let search = view.read(cx).menu.keybinds_search.as_ref().unwrap().clone();
        search.update(cx, |input, cx| {
            gpui::EntityInputHandler::unmark_text(input, window, cx)
        });
    });
    cx.simulate_keystrokes("escape");
}

pub(super) fn check_preferences_scroll(
    view: &Entity<HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) {
    // Exercise the retained modal, not the standalone Settings command.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_preferences_fixture(window, cx));
    });
    // General has enough content to exercise the independent body scroll.
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_resize(size(px(360.), px(240.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let header = cx.debug_bounds("preferences-header").unwrap();
    let footer = cx.debug_bounds("preferences-footer").unwrap();
    let body = cx.debug_bounds("preferences-body").unwrap();
    let usage_row = cx.debug_bounds("preferences-show-usage").unwrap();
    assert!(body.size.height > px(0.));
    assert!(header.bottom() <= body.top());
    assert!(body.bottom() <= footer.top());
    cx.simulate_keystrokes("pagedown");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("preferences-show-usage").unwrap().top() < usage_row.top());
    assert_eq!(cx.debug_bounds("preferences-header").unwrap(), header);
    assert_eq!(cx.debug_bounds("preferences-footer").unwrap(), footer);
    let close = cx.debug_bounds("preferences-close").unwrap();
    cx.simulate_click(close.center(), Default::default());
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
}

pub(super) fn check_github_panel(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    for width in [320., 640., 1200.] {
        cx.simulate_resize(size(px(width), px(400.)));
        for state in 0..5 {
            cx.update(|window, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into()));
                cx.default_global::<PaintedProbes>().0.clear();
                view.update(cx, |view, cx| {
                    view.github_fixture(state == 1, window, cx);
                    if state == 2 {
                        view.menu.github.failed = true;
                        view.menu.github.message =
                            Some("GitHub code expired. Sign in again. ".repeat(40));
                    } else if state == 3 {
                        view.menu.github = crate::github::Auth::connected_fixture();
                    } else if state == 4 {
                        view.menu.github = crate::github::Auth::requesting_fixture();
                    }
                });
                full_draw(window, cx).clear(cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("unchanged")
                );
                assert_eq!(
                    cx.global::<PaintedProbes>().0.contains_key("Sign out (D)"),
                    state == 3
                );
            });
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(panel.bottom() <= px(400.));
            assert!(panel.size.width <= px(400.));
            assert!(cx.debug_bounds("github-close").is_none());
            let close = cx.debug_bounds("github-header-close").unwrap();
            assert!(close.top() >= panel.top() && close.bottom() <= panel.bottom());
            if state == 3 {
                assert!(panel.size.height <= px(230.));
            }
            let footer = cx.debug_bounds("github-footer");
            let body = cx.debug_bounds("github-body").unwrap();
            assert!(body.size.height > px(0.));
            // A pending request offers no footer actions, so none is drawn.
            assert_eq!(footer.is_none(), state == 4);
            if let Some(footer) = footer {
                // Content-sized layouts can round adjacent edges to half pixels.
                assert!(body.bottom() <= footer.top() + px(1.));
                assert!(footer.bottom() <= panel.bottom() + px(1.));
            } else {
                assert!(body.bottom() <= panel.bottom() + px(1.));
            }
            if state == 1 {
                let code = cx.debug_bounds("github-device-code").unwrap();
                assert!(code.left() >= panel.left() && code.right() <= panel.right());
                let copy = cx.debug_bounds("github-copy").unwrap();
                cx.simulate_click(copy.center(), Default::default());
                cx.update(|_, cx| {
                    assert_eq!(
                        cx.read_from_clipboard().unwrap().text().as_deref(),
                        Some("ABCD-1234")
                    );
                    assert!(view.read(cx).menu.github.copied());
                });
                cx.simulate_keystrokes("tab enter");
                cx.update(|_, cx| assert!(view.read(cx).menu.github.copied()));
                cx.simulate_keystrokes("cmd-c");
                let open = cx.debug_bounds("github-open").unwrap();
                cx.simulate_click(open.center(), Default::default());
                assert_eq!(cx.opened_url().as_deref(), Some(crate::github::VERIFY_URL));
            } else if state == 2 {
                let status = cx.debug_bounds("github-status").unwrap();
                cx.simulate_keystrokes("pagedown");
                cx.update(|window, cx| full_draw(window, cx).clear(cx));
                assert!(cx.debug_bounds("github-status").unwrap().top() < status.top());
                assert_eq!(cx.debug_bounds("github-footer"), footer);
            }
            cx.simulate_keystrokes("c escape");
            cx.update(|window, cx| {
                assert!(view.read(cx).menu.page.is_none());
                assert!(view.read(cx).focus.is_focused(window));
                assert!(view.read(cx).menu.github.code().is_none());
                assert!(!view.read(cx).menu.github.copied());
            });
        }
    }
}

pub(super) fn check_theme_picker(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preferences_fixture(window, cx);
            view.select_settings_tab(crate::settings_panel::Tab::Theme, window, cx)
        });
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let choose_theme = cx.debug_bounds("preferences-choose-theme").unwrap();
    cx.simulate_click(choose_theme.center(), Default::default());
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Themes)));
    cx.simulate_keystrokes("escape");

    let search = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_theme_picker(window, cx));
        full_draw(window, cx).clear(cx);
        let search = view.read(cx).menu.themes.as_ref().unwrap().search.clone();
        assert!(search.read(cx).focus.is_focused(window));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("catppuccin mocha".into()));
        search
    });
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(search.read(cx).text(), "catppuccin mocha");
        assert!(view.read(cx).marked.is_empty());
    });
    assert!(cx.debug_bounds("theme-name-Catppuccin Mocha").is_some());
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).menu.themes.as_ref().unwrap().filtered,
            ["Catppuccin Mocha"]
        );
    });
    cx.simulate_keystrokes("cmd-a n o r d");
    cx.run_until_parked();
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(search.read(cx).text(), "nord");
        assert!(
            view.read(cx)
                .menu
                .themes
                .as_ref()
                .unwrap()
                .filtered
                .iter()
                .all(|name| name.to_lowercase().contains("nord"))
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("no-such-theme-xyz".into()))
    });
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("theme-empty").is_some());
    // Enter with no results must neither write a config nor dismiss the picker.
    cx.simulate_keystrokes("down enter");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Themes)));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).focus.is_focused(window));
        view.update(cx, |view, cx| view.open_theme_picker(window, cx));
        full_draw(window, cx).clear(cx);
        assert!(search.read(cx).text().is_empty());
    });
    cx.update(|window, cx| {
        search.update(cx, |search, cx| {
            gpui::EntityInputHandler::replace_and_mark_text_in_range(
                search,
                None,
                "Nord",
                Some(4..4),
                window,
                cx,
            );
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert!(
            view.read(cx).menu.page == Some(crate::menu::Page::Themes),
            "IME confirmation must not apply a theme"
        );
    });
    cx.update(|window, cx| {
        search.update(cx, |search, cx| {
            gpui::EntityInputHandler::unmark_text(search, window, cx)
        });
    });
    cx.simulate_keystrokes("escape");
}

pub(super) fn check_command_palette(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes("cmd-shift-p");
    let palette_search = cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette));
        view.read(cx).menu.palette.as_ref().unwrap().search.clone()
    });
    // Bound native commands must not fire while a search field has focus.
    cx.simulate_keystrokes("cmd-b");
    cx.update(|_, cx| assert!(view.read(cx).sidebar_visible));
    cx.simulate_input("toggle sidebar");
    cx.update(|_, cx| assert_eq!(palette_search.read(cx).text(), "toggle sidebar"));
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(!view.read(cx).sidebar_visible);
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
    cx.simulate_keystrokes("cmd-b");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_preferences_fixture(window, cx));
    });
    cx.update(|_, cx| {
        assert!(view.read(cx).sidebar_visible);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Preferences));
    });
    cx.simulate_keystrokes("escape cmd-p");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette));
        let search = &view.read(cx).menu.palette.as_ref().unwrap().search;
        assert!(search.read(cx).text().is_empty());
    });
    cx.simulate_input("no-workspace-matches-xyz");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette)));
    cx.simulate_keystrokes("escape");
}

pub(super) fn check_close_confirmation(
    view: &Entity<HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.live.snapshot = Some(Arc::new(
                serde_json::from_str(include_str!(
                    "../../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
                ))
                .unwrap(),
            ));
            cx.notify();
        });
    });
    cx.simulate_keystrokes("cmd-w");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::ConfirmClose)));
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert!(
            view.read(cx).menu.page.is_none(),
            "Enter defaults to Cancel"
        )
    });
    cx.simulate_keystrokes("cmd-shift-w tab enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(
            view.read(cx).menu.page == Some(crate::menu::Page::ConfirmClose),
            "disconnected confirmation stays open with error"
        );
        let view = view.read(cx);
        assert!(
            view.endpoints[view.selected_endpoint]
                .connection
                .handle
                .is_none()
        );
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
}

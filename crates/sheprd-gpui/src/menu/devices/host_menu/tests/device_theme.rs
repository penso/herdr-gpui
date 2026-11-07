use super::*;
use crate::{
    config::{DeviceSettings, Theme},
    menu::devices::host_menu::theme::Row,
};
use gpui::Entity;

const PROFILE: &str = "0123456789abcdef0123456789abcdef";

fn builtin(name: &str) -> Theme {
    Theme::builtin(name).unwrap_or_default()
}

/// The saved host's menu, with the host selected and an app theme that no
/// device row matches.
fn open_menu<'a>(
    theme: Option<&str>,
    cx: &'a mut TestAppContext,
) -> (Entity<crate::SheprdWindow>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        add_host(&mut view);
        view.selected_endpoint = 1;
        view.theme = builtin("Nord");
        if let Some(theme) = theme {
            view.config.devices.insert(
                PROFILE.into(),
                DeviceSettings {
                    theme: Some(theme.into()),
                    ..DeviceSettings::default()
                },
            );
        }
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update_in(cx, |view, window, cx| {
        view.open_host_menu(HOST, point(px(10.), px(10.)), window, cx)
    });
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn rows(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) -> Vec<Row> {
    view.read_with(cx, |view, _| {
        host(view)
            .theme
            .as_ref()
            .map(|picker| picker.filtered.clone())
            .unwrap_or_default()
    })
}

fn select(view: &Entity<crate::SheprdWindow>, row: &Row, cx: &mut VisualTestContext) {
    let index = rows(view, cx).iter().position(|candidate| candidate == row);
    view.update(cx, |view, cx| {
        if let Some(picker) = view.menu.host.as_mut().and_then(|host| host.theme.as_mut()) {
            picker.selected = index.unwrap_or_default();
        }
        view.preview_device_theme(cx);
    });
    cx.run_until_parked();
}

fn terminal_theme(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) -> Theme {
    view.read_with(cx, |view, _| view.terminal_theme().clone())
}

#[gpui::test]
fn the_picker_previews_on_the_terminal_and_escape_restores_it(cx: &mut TestAppContext) {
    let (view, cx) = open_menu(None, cx);
    // Nothing set: the row shows no value and the picker starts on the app.
    assert!(cx.debug_bounds("host-menu-theme").is_none());
    let theme = SSH_ACTIONS
        .iter()
        .position(|(action, _)| *action == Action::Theme)
        .unwrap();
    cx.simulate_keystrokes(&format!("{}enter", "down ".repeat(theme + 1)));
    assert!(view.read_with(cx, |view, _| view.menu.page == Some(Page::DeviceTheme)));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("device-theme").is_some());
    assert_eq!(rows(&view, cx).first(), Some(&Row::App));
    view.read_with(cx, |view, _| {
        assert_eq!(
            host(view).theme.as_ref().unwrap().selected_row(),
            Some(&Row::App)
        );
    });

    select(&view, &Row::Named("Dracula".into()), cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
    // Only the terminal previews; the chrome keeps the app theme.
    view.read_with(cx, |view, _| assert_eq!(view.theme, builtin("Nord")));
    select(&view, &Row::App, cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_ne!(terminal_theme(&view, cx), builtin("Nord"));

    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
}

#[gpui::test]
fn the_picker_starts_on_the_saved_theme_and_filters_by_name(cx: &mut TestAppContext) {
    let (view, cx) = open_menu(Some("light:Catppuccin Latte,dark:Dracula"), cx);
    // The row names the saved value.
    assert!(cx.debug_bounds("host-menu-theme").is_some());
    view.update_in(cx, |view, window, cx| {
        view.activate_host_menu(Action::Theme, window, cx)
    });
    cx.run_until_parked();
    let light = cx.update(|_, cx| crate::app::light_appearance(cx));
    let shown = Row::Named(if light { "Catppuccin Latte" } else { "Dracula" }.into());
    view.read_with(cx, |view, _| {
        assert_eq!(
            host(view).theme.as_ref().unwrap().selected_row(),
            Some(&shown)
        );
    });

    cx.simulate_input("mocha");
    assert_eq!(rows(&view, cx), [Row::Named("Catppuccin Mocha".into())]);
    cx.simulate_input("x");
    assert!(rows(&view, cx).is_empty());
    draw(cx);
    assert!(cx.debug_bounds("device-theme").is_some());
}

#[gpui::test]
fn another_device_previews_nothing_on_the_shown_terminal(cx: &mut TestAppContext) {
    let (view, cx) = open_menu(None, cx);
    view.update_in(cx, |view, window, cx| {
        view.selected_endpoint = 0;
        view.activate_host_menu(Action::Theme, window, cx)
    });
    select(&view, &Row::Named("Dracula".into()), cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.device_theme_preview(HOST), Some(&builtin("Dracula")));
    });
}

#[gpui::test]
fn a_theme_file_that_cannot_load_is_not_saved(cx: &mut TestAppContext) {
    let (view, cx) = open_menu(Some("/herdr-gpui-test/missing-theme"), cx);
    view.update_in(cx, |view, window, cx| {
        view.activate_host_menu(Action::Theme, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        // The picker stays open, says why, and nothing is being saved.
        assert_eq!(view.menu.page, Some(Page::DeviceTheme));
        assert!(host(view).theme.as_ref().unwrap().error.is_some());
        assert_eq!(
            view.device_theme_value(HOST),
            Some("/herdr-gpui-test/missing-theme")
        );
    });
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
}

#[gpui::test]
fn the_theme_list_belongs_to_the_picker_that_asked(cx: &mut TestAppContext) {
    let (view, cx) = open_menu(None, cx);
    view.update_in(cx, |view, window, cx| {
        view.activate_host_menu(Action::Theme, window, cx)
    });
    // Listing runs with the picker and is dropped with it, so a later
    // picker never takes an earlier one's list.
    view.read_with(cx, |view, _| {
        assert!(host(view).theme.as_ref().unwrap().discovery.is_some());
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(host(view).theme.as_ref().unwrap().discovery.is_none());
    });
}

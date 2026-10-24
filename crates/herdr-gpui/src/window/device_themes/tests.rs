use super::*;
use crate::{config::DeviceSettings, sidebar::layout_tests::fixture_window};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

const PROFILE: &str = "0123456789abcdef0123456789abcdef";
const HOST: &str = "ssh:0123456789abcdef0123456789abcdef";

fn builtin(name: &str) -> Theme {
    Theme::builtin(name).unwrap_or_default()
}

/// A window with Local and one saved device, the device selected.
fn open(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
        view.endpoints.push(crate::endpoint::Endpoint::new(
            HOST.into(),
            "box".into(),
            herdr_client::ConnectTarget::Ssh {
                target: "penso@box".into(),
                session: "default".into(),
            },
            true,
        ));
        view.selected_endpoint = 1;
        view.theme = builtin("Nord");
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

fn set_device_theme(view: &Entity<HerdrWindow>, theme: Option<&str>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        view.config.devices.insert(
            PROFILE.into(),
            DeviceSettings {
                theme: theme.map(str::to_owned),
                ..DeviceSettings::default()
            },
        );
    });
}

/// Draws, lets the background read finish, and draws what it applied.
fn settle(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn terminal_theme(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Theme {
    view.read_with(cx, |view, _| view.terminal_theme().clone())
}

#[gpui::test]
fn the_selected_device_paints_its_terminal_with_its_own_theme(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    settle(cx);
    // No device theme: the terminal is the app theme.
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));

    set_device_theme(&view, Some("Dracula"), cx);
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
    view.read_with(cx, |view, _| {
        // The chrome keeps the app theme.
        assert_eq!(view.theme, builtin("Nord"));
        assert_eq!(view.device_themes.get(PROFILE), Some(&builtin("Dracula")));
    });

    // Local is not a saved device: selecting it shows the app theme.
    view.update(cx, |view, _| view.selected_endpoint = 0);
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
    view.update(cx, |view, _| view.selected_endpoint = 1);
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));

    // Removing the theme stops at once, before any read.
    set_device_theme(&view, None, cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
}

#[gpui::test]
fn a_device_theme_that_cannot_load_keeps_the_app_theme(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    set_device_theme(&view, Some("../not-a-theme"), cx);
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Nord"));
    // Fixing it reads again.
    set_device_theme(&view, Some("Catppuccin Mocha"), cx);
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Catppuccin Mocha"));
}

/// A config load that never finishes on its own, as one already running
/// when a save lands. It also keeps the save's reload from reading the
/// user's real config.
fn busy_load(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.config_load = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
    });
}

/// A config load landing, its config holding `theme` for the device.
fn reload(view: &Entity<HerdrWindow>, theme: Option<&str>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        view.config_load = None;
        view.config_load_revision += 1;
    });
    set_device_theme(view, theme, cx);
}

fn save(
    view: &Entity<HerdrWindow>,
    name: &str,
    write: crate::Result<()>,
    cx: &mut VisualTestContext,
) {
    view.update(cx, |view, cx| {
        view.save_device_theme_with(
            HOST.into(),
            Some(name.into()),
            Some(builtin(name)),
            move || write,
            cx,
        );
    });
}

fn pinned(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.device_themes.pinned.is_some())
}

#[gpui::test]
fn a_saved_choice_shows_until_a_reload_after_its_write_lands(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    settle(cx);
    busy_load(&view, cx);
    save(&view, "Dracula", Ok(()), cx);
    // Shown at once, while the write runs.
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
    view.read_with(cx, |view, _| {
        assert_eq!(view.device_theme_value(HOST), Some("Dracula"));
    });
    settle(cx);

    // The load that was already running read the file before the write:
    // its old value does not take the choice back.
    reload(&view, None, cx);
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
    assert!(pinned(&view, cx));

    // The next reload holds the saved value; the pin goes once it is read.
    reload(&view, Some("Dracula"), cx);
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
    assert!(!pinned(&view, cx));
}

#[gpui::test]
fn a_failed_save_drops_its_choice_and_says_why(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    set_device_theme(&view, Some("Dracula"), cx);
    settle(cx);
    save(
        &view,
        "Nord",
        Err(crate::config::Error::InvalidSidebarGap.into()),
        cx,
    );
    settle(cx);
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
    assert!(!pinned(&view, cx));
    view.read_with(cx, |view, _| {
        let error = view.local_error.as_deref().unwrap_or_default();
        assert!(error.starts_with("Save GUI config:"), "{error}");
    });
}

#[gpui::test]
fn a_later_reload_with_another_value_drops_the_choice(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    set_device_theme(&view, Some("Dracula"), cx);
    settle(cx);
    busy_load(&view, cx);
    save(&view, "Catppuccin Latte", Ok(()), cx);
    settle(cx);
    reload(&view, Some("Dracula"), cx);
    settle(cx);
    assert!(pinned(&view, cx));
    // Edited again since: the file wins rather than the pin hiding it.
    reload(&view, Some("Dracula"), cx);
    settle(cx);
    assert!(!pinned(&view, cx));
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
}

#[gpui::test]
#[allow(clippy::unwrap_used)]
fn every_reload_reads_the_theme_files_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device-theme");
    std::fs::write(&path, "background = #102030\n").unwrap();
    let value = path.to_string_lossy().into_owned();
    let (view, cx) = open(cx);
    set_device_theme(&view, Some(&value), cx);
    settle(cx);
    let before = terminal_theme(&view, cx).background;
    assert_ne!(before, builtin("Nord").background);

    // Edited in place: an unrelated reload picks the new colors up.
    std::fs::write(&path, "background = #405060\n").unwrap();
    reload(&view, Some(&value), cx);
    settle(cx);
    let after = terminal_theme(&view, cx).background;
    assert_ne!(after, before);
    assert_ne!(after, builtin("Nord").background);
}

#[gpui::test]
fn a_running_load_that_already_holds_the_choice_releases_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    settle(cx);
    // The watcher started this load after the write, so it holds the
    // saved value and no further reload follows.
    busy_load(&view, cx);
    save(&view, "Dracula", Ok(()), cx);
    settle(cx);
    reload(&view, Some("Dracula"), cx);
    settle(cx);
    assert!(!pinned(&view, cx));
    assert_eq!(terminal_theme(&view, cx), builtin("Dracula"));
}

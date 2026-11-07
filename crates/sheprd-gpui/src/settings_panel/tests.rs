use super::*;
use core::prelude::v1::test;

#[gpui::test]
#[allow(clippy::unwrap_used)]
fn followed_theme_updates_startup_cache_without_persisting_session_font_size(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let shared = Settings::parse_text("[theme]\nname = 'nord'").unwrap();
    let expected = shared.theme(false).unwrap();
    view.update(cx, |view, cx| {
        view.settings.shared = Some(shared);
        view.load_gui_config_with(
            || {
                Ok((
                    crate::config::Config {
                        theme: "Follow Herdr".into(),
                        ..Default::default()
                    },
                    Default::default(),
                ))
            },
            cx,
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.theme, expected);
        assert_eq!(cx.global::<crate::app::InitialAppearance>().theme, expected);
    });
    let shared = Settings::parse_text("[theme]\nname = 'dracula'").unwrap();
    let expected = shared.theme(false).unwrap();
    view.update(cx, |view, cx| {
        let saved_size = cx
            .global::<crate::app::InitialAppearance>()
            .config
            .terminal
            .size;
        view.config.terminal.size = 28.;
        view.settings.shared = Some(shared);
        view.apply_shared_theme(cx);
        let appearance = cx.global::<crate::app::InitialAppearance>();
        assert_eq!(appearance.theme, expected);
        assert_eq!(appearance.config.terminal.size, saved_size);
    });
}

#[gpui::test]
#[allow(clippy::unwrap_used)]
fn followed_theme_applies_contrast_to_a_fresh_palette(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        let shared = Settings::parse_text("[theme]\nname = 'nord'").unwrap();
        let light = matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let fresh = shared.theme(light).unwrap();
        view.settings.shared = Some(shared);
        view.config.theme = "Follow Herdr".into();
        for contrast in [
            crate::contrast::Contrast::High,
            crate::contrast::Contrast::High,
            crate::contrast::Contrast::Standard,
        ] {
            view.config.contrast = contrast;
            view.apply_shared_theme(cx);
            assert_eq!(view.theme, fresh.clone().with_contrast(contrast));
        }
    });
}

/// Herdr's `ui.agent_panel_sort` picks the order, also on a config reload,
/// until a toggle, now or stored from an earlier run, overrides it.
#[gpui::test]
#[allow(clippy::unwrap_used)]
fn the_daemon_sort_seeds_the_agents_panel_until_toggled(cx: &mut TestAppContext) {
    use crate::preferences::{AgentSort, Chrome};
    let (view, visual) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let load = |cx: &mut VisualTestContext, sort: &'static str| {
        view.update(cx, |view, cx| {
            view.load_shared_settings_with(
                move || {
                    Ok(Settings::parse_text(&format!(
                        "[ui]\nagent_panel_sort = '{sort}'"
                    ))?)
                },
                cx,
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| (view.agent_sort, view.agent_sort_modified))
    };
    assert_eq!(load(visual, "priority"), (AgentSort::Priority, false));
    // Stored chrome without a toggle leaves the daemon's choice alone.
    view.update(visual, |view, _| {
        view.apply_stored_chrome(Chrome::default())
    });
    assert_eq!(load(visual, "priority"), (AgentSort::Priority, false));
    assert_eq!(load(visual, "spaces"), (AgentSort::Grouped, false));
    assert_eq!(load(visual, "priority"), (AgentSort::Priority, false));
    // A toggle is the user's from then on; a reload no longer moves it.
    view.update(visual, |view, _| {
        view.agent_sort = view.agent_sort.toggled();
        view.agent_sort_modified = true;
    });
    assert_eq!(load(visual, "priority"), (AgentSort::Grouped, true));

    // A toggle stored by an earlier run wins whichever arrives first.
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.load_shared_settings_with(
            || Ok(Settings::parse_text("[ui]\nagent_panel_sort = 'priority'")?),
            cx,
        );
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        view.apply_stored_chrome(Chrome {
            agent_sort: Some(AgentSort::Grouped),
            ..Chrome::default()
        });
        assert_eq!(view.agent_sort, AgentSort::Grouped);
        assert!(view.agent_sort_modified);
    });
}

#[gpui::test]
fn first_shared_load_honors_sidebar_start_collapsed_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let load =
        || Settings::parse_text("[ui]\nsidebar_start_collapsed = true\n").map_err(Into::into);
    view.update(cx, |view, cx| view.load_shared_settings_with(load, cx));
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert!(!view.sidebar_visible);
        view.toggle_sidebar();
        // A config reload is not a startup.
        view.load_shared_settings_with(load, cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.sidebar_visible));
}

#[gpui::test]
fn failed_load_is_bounded_and_keeps_native_appearance(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.load_shared_settings_with(|| Err(crate::Error::MissingHome), cx);
        view.load_shared_settings_with(|| panic!("only one shared load at a time"), cx);
        assert!(view.settings.task.is_some());
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.settings.loaded);
        assert!(view.settings.task.is_none());
        assert!(view.settings.error.is_some());
        assert_eq!(view.config.theme, "Default");
    });
}

#[gpui::test]
fn gui_config_load_applies_the_loaded_fonts(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        let mut config = view.config.clone();
        config.sidebar.family = "Loaded Sidebar".into();
        config.sidebar.size = 19.;
        view.load_gui_config_with(move || Ok((config, Default::default())), cx);
        // Completion cannot run until this update returns. No disk writes are needed.
        assert!(view.config_load.is_some());
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.config_load.is_none());
        assert_eq!(view.config.sidebar.family, "Loaded Sidebar");
        assert_eq!(view.config.sidebar.size, 19.);
    });
}

mod appearance_report;
mod system_theme;
mod theme_reload;

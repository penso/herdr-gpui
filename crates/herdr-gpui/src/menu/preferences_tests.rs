//! The Preferences Herdr Projects rows: the switch, the mandatory folder, the
//! plugin status, and the install offer. These live beside the menu because
//! they reach its own pages and key routing.

#![allow(clippy::unwrap_used)]

use gpui::{Entity, Modifiers, VisualTestContext, point, px};
use std::path::{Path, PathBuf};

use crate::{HerdrWindow, sidebar::layout_tests::fixture_window};

fn root() -> &'static str {
    if cfg!(windows) {
        "C:/fixture/projects"
    } else {
        "/fixture/projects"
    }
}

fn root_child() -> &'static str {
    if cfg!(windows) {
        "C:/fixture/projects/new"
    } else {
        "/fixture/projects/new"
    }
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, scroll: f32) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preferences(window, cx);
            view.menu
                .preferences_scroll
                .set_offset(point(px(0.), px(-scroll)));
        });
        window.draw(cx).clear(cx);
    });
}

fn click_edit(cx: &mut VisualTestContext) {
    let edit = cx.debug_bounds("preferences-projects-root-edit").unwrap();
    cx.simulate_click(edit.center(), Modifiers::default());
}

fn set_text(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, text: &str) {
    view.update(cx, |view, cx| {
        let editor = view.menu.projects_root_editor.as_ref().unwrap();
        editor
            .input
            .update(cx, |input, cx| input.set_text_selected(text, cx));
    });
}

#[gpui::test]
fn preferences_projects_root_edits_a_valid_value_and_clears_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        view.config.projects_root = Some(PathBuf::from(root()));
        // A real writer would touch the user's config file; holding the loader
        // keeps this test's commits queued instead.
        view.config_load = Some(cx.spawn(async |_, _| std::future::pending().await));
    });
    open(&view, cx, 140.);
    assert!(cx.debug_bounds("preferences-projects-root-edit").is_some());

    click_edit(cx);
    view.read_with(cx, |view, _| {
        assert!(view.menu.projects_root_editor.is_some())
    });
    set_text(&view, cx, root_child());
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.menu.projects_root_editor.is_none());
        assert_eq!(
            view.config.projects_root.as_deref(),
            Some(Path::new(root_child()))
        );
        // Queued, not written, so no error is reported.
        assert!(
            view.settings_saves
                .status()
                .is_some_and(|s| s.contains("Saving"))
        );
    });

    // Clearing the field disables the section.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_edit(cx);
    set_text(&view, cx, "");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.menu.projects_root_editor.is_none());
        assert!(view.config.projects_root.is_none());
    });
}

#[gpui::test]
fn preferences_projects_root_rejects_a_relative_path_and_cancels_on_escape(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.config.projects_root = Some(PathBuf::from(root()));
    });
    open(&view, cx, 140.);

    // A relative path is refused before anything is written.
    click_edit(cx);
    set_text(&view, cx, "relative/projects");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.menu.projects_root_editor.is_none());
        assert_eq!(
            view.config.projects_root.as_deref(),
            Some(Path::new(root()))
        );
        assert!(
            view.settings_saves
                .status()
                .is_some_and(|status| status.contains("projects_root"))
        );
    });

    // Escape abandons the edit without saving.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click_edit(cx);
    set_text(&view, cx, root_child());
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(view.menu.projects_root_editor.is_none());
        assert_eq!(
            view.config.projects_root.as_deref(),
            Some(Path::new(root()))
        );
        assert_eq!(view.menu.page, Some(super::Page::Preferences));
    });
}

#[gpui::test]
fn preferences_switch_defaults_the_projects_folder(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        view.config_load = Some(cx.spawn(async |_, _| std::future::pending().await));
    });
    open(&view, cx, 140.);
    let toggle = cx
        .debug_bounds("preferences-use-herdr-projects-toggle")
        .unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(view.config.use_herdr_projects);
        // The switch is mandatory with a folder, so the documented default fills in.
        assert!(view.config.projects_root.is_some());
        assert!(view.config.herdr_projects_enabled());
        assert!(view.settings_saves.status().is_some());
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let toggle = cx
        .debug_bounds("preferences-use-herdr-projects-toggle")
        .unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    view.read_with(cx, |view, _| assert!(!view.config.use_herdr_projects));
}

#[gpui::test]
fn preferences_header_height_stepper_changes_and_saves(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        // Keep the write queued instead of touching the user's config file.
        view.config_load = Some(cx.spawn(async |_, _| std::future::pending().await));
    });
    open(&view, cx, 0.);
    let before = view.read_with(cx, |view, _| view.config.layout.sidebar_header_height);
    let increase = cx
        .debug_bounds("preferences-sidebar-header-height-increase")
        .unwrap();
    cx.simulate_click(increase.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.config.layout.sidebar_header_height, before + 1.);
        assert!(view.settings_saves.status().is_some());
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let decrease = cx
        .debug_bounds("preferences-sidebar-header-height-decrease")
        .unwrap();
    cx.simulate_click(decrease.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.config.layout.sidebar_header_height, before)
    });
}

#[gpui::test]
fn clicking_the_space_button_attempts_a_project_creation(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.use_herdr_projects = true;
            view.config.projects_root = Some(PathBuf::from(root()));
            // A binary that cannot exist, so the spawn fails without touching
            // the real plugin.
            view.herdr_projects.installed = Some(crate::herdr_projects::Installed {
                version: "1.0".into(),
                root: PathBuf::from("/nonexistent-plugin-root"),
            });
            cx.notify();
        });
        window.draw(cx).clear(cx);
    });
    let plus = cx.debug_bounds("project-new-w0").unwrap();
    cx.simulate_click(plus.center(), Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(
            view.local_error
                .as_deref()
                .is_some_and(|error| error.contains("Could not create the project")),
            "unexpected error: {:?}",
            view.local_error
        );
    });
}

#[gpui::test]
fn preferences_offer_install_only_when_the_plugin_is_missing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preferences(window, cx);
            // Cancel the real detection and stand in a finished, empty result.
            view.herdr_projects.detect = None;
            view.herdr_projects.checked = true;
            view.herdr_projects.installed = None;
            view.menu
                .preferences_scroll
                .set_offset(point(px(0.), px(-140.)));
        });
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("preferences-install-herdr-projects")
            .is_some()
    );

    view.update(cx, |view, cx| {
        view.herdr_projects.installed = Some(crate::herdr_projects::Installed {
            version: "1.0".into(),
            root: PathBuf::from("/p"),
        });
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("preferences-install-herdr-projects")
            .is_none()
    );
}

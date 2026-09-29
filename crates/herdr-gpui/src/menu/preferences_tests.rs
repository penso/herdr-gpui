//! The Preferences `projects_root` row: the inline editor, its validation, and
//! how a committed value reaches the config. These live beside the menu because
//! they reach its own pages and key routing.

#![allow(clippy::unwrap_used)]

use gpui::{Entity, Modifiers, VisualTestContext};
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
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.projects_root = Some(PathBuf::from(root()));
            // A real writer would touch the user's config file; holding the
            // loader keeps this test's commits queued instead.
            view.config_load = Some(cx.spawn(async |_, _| std::future::pending().await));
            view.open_preferences(window, cx);
        });
        window.draw(cx).clear(cx);
    });
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
            view.projects_root_save
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
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.projects_root = Some(PathBuf::from(root()));
            view.open_preferences(window, cx);
        });
        window.draw(cx).clear(cx);
    });

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
            view.projects_root_save
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

//! Zen mode hides the chrome and every group but the one in use, and leaving
//! it brings back exactly the window that was there.

#![allow(clippy::unwrap_used)]

use super::Zen;
use crate::config::ZenWidth;
use crate::{
    HerdrWindow,
    controls::Command,
    sidebar::layout_tests::{fixture_window, full_draw, snapshot},
};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
use std::sync::Arc;

fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(4);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    });
    cx.simulate_resize(size(px(1000.), px(600.)));
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn run(view: &Entity<HerdrWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    draw(cx);
}

fn drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

/// The header shows as its own bar, or merged into the sidebar's column and
/// the leftmost strip; either way some part of it is drawn.
fn header_drawn(cx: &mut VisualTestContext) -> bool {
    drawn(cx, "titlebar-center") || drawn(cx, "sidebar-titlebar")
}

fn fullscreen(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.is_fullscreen())
}

#[gpui::test]
fn zen_hides_the_chrome_and_leaving_restores_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let terminal = cx.debug_bounds("terminal").unwrap();
    for selector in ["sidebar", "tab-t0", "connection-status"] {
        assert!(drawn(cx, selector), "{selector}");
    }
    assert!(header_drawn(cx));
    assert!(!drawn(cx, "zen-inset"));

    run(&view, Command::ToggleZenMode, cx);
    assert!(view.read_with(cx, |view, _| view.zen_active()));
    for selector in ["sidebar", "tab-t0", "connection-status"] {
        assert!(!drawn(cx, selector), "{selector}");
    }
    assert!(!header_drawn(cx));
    // Only macOS keeps a band for the traffic lights; the test window's
    // frame is server-drawn, so no other platform needs one.
    assert_eq!(drawn(cx, "zen-inset"), cfg!(target_os = "macos"));
    let zen = cx.debug_bounds("terminal").unwrap();
    assert!(zen.size.width > terminal.size.width, "{zen:?} {terminal:?}");
    assert!(
        zen.size.height > terminal.size.height,
        "{zen:?} {terminal:?}"
    );
    assert_eq!(zen.left(), px(0.));

    run(&view, Command::ToggleZenMode, cx);
    assert!(!view.read_with(cx, |view, _| view.zen_active()));
    assert_eq!(cx.debug_bounds("terminal").unwrap(), terminal);
    assert!(drawn(cx, "sidebar"));
    assert!(drawn(cx, "connection-status"));
    assert!(header_drawn(cx));
}

/// Zen mode reads over the sidebar and status bar toggles rather than
/// changing them, so a toggle made while hidden shows once it ends.
#[gpui::test]
fn toggles_made_in_zen_mode_apply_when_it_ends(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    run(&view, Command::ToggleZenMode, cx);
    run(&view, Command::ToggleSidebar, cx);
    run(&view, Command::ToggleStatusBar, cx);
    assert!(!drawn(cx, "connection-status"));

    run(&view, Command::ToggleZenMode, cx);
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_visible);
        assert!(!view.status_bar_visible);
    });
    assert!(!drawn(cx, "connection-status"));
    assert!(!drawn(cx, "sidebar"));
}

#[gpui::test]
fn zen_draws_only_the_group_in_use(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    run(&view, Command::SplitEditor, cx);
    let (slots, active) = view.read_with(cx, |view, _| (view.group_slots(), view.active_group()));
    assert_eq!(slots.len(), 2);
    assert!(drawn(cx, "group-divider-0"));

    run(&view, Command::ToggleZenMode, cx);
    let kept = view.read_with(cx, |view, _| view.drawn_group_slots());
    assert_eq!(kept.len(), 1);
    assert_eq!(Some(kept[0].id), active);
    assert!(!drawn(cx, "group-divider-0"));
    let hidden = slots.iter().find(|slot| Some(slot.id) != active).unwrap();
    let hidden = Box::leak(hidden.selector("group").into_boxed_str());
    assert!(!drawn(cx, hidden));
    let group = Box::leak(kept[0].selector("group").into_boxed_str());
    let width = cx.update(|window, _| window.viewport_size().width);
    assert_eq!(cx.debug_bounds(group).unwrap().size.width, width);

    // The layout itself is untouched, so both groups come back.
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(view.read_with(cx, |view, _| view.group_slots()), slots);
    assert!(drawn(cx, hidden));
    assert!(drawn(cx, "group-divider-0"));
}

#[gpui::test]
fn zen_enters_full_screen_only_when_configured(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    run(&view, Command::ToggleZenMode, cx);
    assert!(!fullscreen(cx));
    run(&view, Command::ToggleZenMode, cx);

    view.update(cx, |view, _| view.config.zen.fullscreen = true);
    run(&view, Command::ToggleZenMode, cx);
    assert!(fullscreen(cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.zen.mode),
        Zen::On {
            entered_fullscreen: true,
            restore_font: None,
        }
    );
    // Full screen leaves no traffic lights to clear.
    assert!(!drawn(cx, "zen-inset"));
    run(&view, Command::ToggleZenMode, cx);
    assert!(!fullscreen(cx));
}

/// A window already full screen stays so after zen mode, as does one the
/// user took out of full screen while in it.
#[gpui::test]
fn zen_leaves_full_screen_it_did_not_enter(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| view.config.zen.fullscreen = true);
    cx.update(|window, _| window.toggle_fullscreen());
    run(&view, Command::ToggleZenMode, cx);
    run(&view, Command::ToggleZenMode, cx);
    assert!(fullscreen(cx));

    cx.update(|window, _| window.toggle_fullscreen());
    run(&view, Command::ToggleZenMode, cx);
    cx.update(|window, _| window.toggle_fullscreen());
    run(&view, Command::ToggleZenMode, cx);
    assert!(!fullscreen(cx));
}

#[gpui::test]
fn entering_zen_says_how_to_leave(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    run(&view, Command::ToggleZenMode, cx);
    let (text, key) = view.read_with(cx, |view, _| {
        (
            view.flash.as_ref().map(|(flash, _)| flash.text.clone()),
            view.keymap().primary(Command::ToggleZenMode).to_owned(),
        )
    });
    assert!(!key.is_empty());
    assert_eq!(
        text.unwrap(),
        format!("Zen mode: \u{2068}{key}\u{2069} to leave")
    );
}

fn set_width(view: &Entity<HerdrWindow>, width: ZenWidth, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| view.config.zen.width = Some(width));
}

/// `[zen] width` in columns centres a column the terminal resizes to, with
/// the same room either side.
#[gpui::test]
fn a_width_in_columns_centres_a_terminal_that_wide(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    set_width(&view, ZenWidth::Columns(80), cx);
    run(&view, Command::ToggleZenMode, cx);
    let column = cx.debug_bounds("zen-column").unwrap();
    let width = cx.update(|window, _| window.viewport_size().width);
    assert!(
        (column.left() - (width - column.right())).abs() <= px(1.),
        "{column:?}"
    );
    assert_eq!(
        cx.debug_bounds("terminal").unwrap().size.width,
        column.size.width
    );
    let cols = view.read_with(cx, |view, _| view.options.surface_size.cols);
    assert_eq!(cols, 80);

    run(&view, Command::ToggleZenMode, cx);
    assert!(!drawn(cx, "zen-column"));
    assert!(view.read_with(cx, |view, _| view.options.surface_size.cols) > 80);
}

#[gpui::test]
fn a_width_in_percent_takes_that_share_of_the_window(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    set_width(&view, ZenWidth::Percent(50), cx);
    run(&view, Command::ToggleZenMode, cx);
    let column = cx.debug_bounds("zen-column").unwrap();
    assert_eq!(column.size.width, px(500.));
    assert_eq!(column.left(), px(250.));
}

/// A window narrower than the column gives the terminal all it has rather
/// than pushing it past the edges.
#[gpui::test]
fn a_column_wider_than_the_window_fits_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    set_width(&view, ZenWidth::Columns(1000), cx);
    run(&view, Command::ToggleZenMode, cx);
    let column = cx.debug_bounds("zen-column").unwrap();
    assert_eq!(column.left(), px(0.));
    assert_eq!(column.size.width, px(1000.));
}

/// Like zen-mode.nvim's terminal font option, `[zen] font_size` holds while
/// in zen mode, and leaving brings back the size from before, even one
/// changed meanwhile.
#[gpui::test]
fn zen_font_size_applies_until_zen_mode_ends(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let size = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.config.terminal.size);
    let before = size(cx);
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(size(cx), before);
    run(&view, Command::ToggleZenMode, cx);

    view.update(cx, |view, _| view.config.zen.font_size = Some(24.));
    let cols = view.read_with(cx, |view, _| view.options.surface_size.cols);
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(size(cx), 24.);
    // The larger face fits fewer columns, and the terminal says so.
    assert!(view.read_with(cx, |view, _| view.options.surface_size.cols) < cols);
    run(&view, Command::IncreaseFontSize, cx);
    assert!(size(cx) > 24.);
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(size(cx), before);
    assert_eq!(
        view.read_with(cx, |view, _| view.options.surface_size.cols),
        cols
    );
}

mod focus;
mod opening;
mod padding;
/// The sidebar and status bar sliding away and back.
mod slide;

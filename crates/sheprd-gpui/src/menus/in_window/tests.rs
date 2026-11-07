#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::{fixture_window, full_draw};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = fixture_window(window, cx);
        view.focus.focus(window, cx);
        view
    })
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

#[cfg(any(target_os = "linux", windows))]
mod bar;
mod config_change;
#[cfg(any(target_os = "linux", windows))]
mod dragging;
#[cfg(any(target_os = "linux", windows))]
mod hidden;
mod navigation;
mod no_bar_to_toggle;
#[cfg(any(target_os = "linux", windows))]
mod resizing;
#[cfg(any(target_os = "linux", windows))]
mod shortcut_labels;

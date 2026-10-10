#![allow(clippy::expect_used, clippy::unwrap_used)]

mod add_device;
mod clock;
mod history;
mod model;
mod narrow_layout;
mod page;

use crate::{
    HerdrWindow,
    controls::Command,
    sidebar::layout_tests::{fixture_window, full_draw, snapshot},
    state::ConnectionStatus,
};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

/// Connected fixture showing workspace `w0` with two working agents.
fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view.live.status = ConnectionStatus::Connected;
        view
    })
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::DevicesOverview, window, cx);
        })
    });
    draw(cx);
}

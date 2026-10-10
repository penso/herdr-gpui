#![allow(clippy::unwrap_used)]

use super::{Location, Store, WebUrl, view::scope};
use crate::{
    HerdrWindow,
    sidebar::layout_tests::{fixture_window, full_draw, snapshot},
};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn url(value: &str) -> Location {
    Location::Web {
        url: WebUrl::try_from(value).unwrap(),
    }
}

/// The fixture window, showing workspace `w0` as a connected daemon would.
fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    })
}

/// Needs a build that shows pages: elsewhere a new tab opens nothing.
#[cfg(any(target_os = "macos", windows))]
mod embedded;

/// Editor groups. Blank browser tabs need no native page, so none is
/// created, and a split alone needs no page at all.
mod groups;

/// Notes need a page to annotate, which Linux builds do not show.
#[cfg(any(target_os = "macos", windows))]
mod notes;

/// Middle-clicking a strip's tab closes it.
mod middle_click;

/// Pages a control request opens, only in a workspace the window shows.
#[cfg(unix)]
mod requests;

/// Browser tabs in a group's strip: reordering and growing in.
mod tab_strip;

/// The VS Code tab's server checks. No native page is created, so this
/// runs on every platform, except the checks only builds that show pages
/// make.
mod code;

/// VS Code as a tab of the groups, opened in the group in use.
mod code_tab;

/// A new VS Code server moves the VS Code tabs to it. Only builds that
/// show pages follow a server at all.
#[cfg(any(target_os = "macos", windows))]
mod code_group_server;

/// A new VS Code page opens on its workspace's folder, once its server
/// answers, which only builds that show pages ask.
#[cfg(any(target_os = "macos", windows))]
mod code_folder;

/// The VS Code server the app starts: its license, its states, and when it
/// starts. Only builds that show pages start one.
#[cfg(any(target_os = "macos", windows))]
mod code_start;

/// Pages step aside under the status bar's tooltips; only builds that show
/// pages have any.
#[cfg(any(target_os = "macos", windows))]
mod status_tooltip;

/// Pages step aside for toasts and the file-transfer card, which they
/// would hide; only builds that show pages have any.
#[cfg(any(target_os = "macos", windows))]
mod overlays;

/// What closing a workspace does to its browser tabs.
mod workspace_close;

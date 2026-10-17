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

/// A page's own icon on its tab; only builds that show pages get one.
#[cfg(any(target_os = "macos", windows))]
mod favicon;

/// Editor groups. Blank browser tabs need no native page, so none is
/// created, and a split alone needs no page at all.
mod groups;

/// Notes need a page to annotate, which Linux builds do not show.
#[cfg(any(target_os = "macos", windows))]
mod notes;

/// Results a local page sends with `herdr.send`, queued as notes. Notes
/// belong to pages, which Linux builds do not show.
#[cfg(any(target_os = "macos", windows))]
mod page_results;

/// Middle-clicking a strip's tab closes it.
mod middle_click;

/// Pages a control request opens, only in a workspace the window shows.
mod requests;

/// Notes a control request sends, only to a pane the window shows.
mod requested_notes;

/// Browser tabs in a group's strip: reordering and growing in.
mod tab_strip;

/// Next Tab, Previous Tab, and tabs by number step through pages too. Needs
/// a build that shows pages: elsewhere a new tab opens nothing.
#[cfg(any(target_os = "macos", windows))]
mod tab_steps;

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

/// Pages hide under the status bar's tooltips where they draw above the
/// window, which only Windows does.
#[cfg(windows)]
mod status_tooltip;

/// Pages hide under toasts and the file-transfer card where they draw above
/// the window, which only Windows does.
#[cfg(windows)]
mod overlays;

/// What closing a workspace does to its browser tabs.
mod workspace_close;

/// The font size keys zoom a browser tab's page. Only builds that show
/// pages open a browser tab.
#[cfg(any(target_os = "macos", windows))]
mod zoom;

/// Terminal padding beside a page; only builds that show pages open one.
#[cfg(any(target_os = "macos", windows))]
mod terminal_padding;

//! The window's menu popups: the pages they show, the state they own, and the
//! workspace operations they carry to the daemon. Input is isolated to the
//! popup while one is open, and every modal action is fenced by the connection
//! it was started under.

mod appear;
mod checkpoints;
mod chrome;
#[cfg(feature = "cloud")]
mod cloud_create;
#[cfg(feature = "cloud")]
mod cloud_destroy;
mod colors;
pub(crate) mod devices;
mod fan_out;
mod git;
mod github;
pub(crate) mod gitlab;
mod open_checkout;
mod open_link;
mod page;
mod page_title;
mod pr;
mod pr_actions;
mod sessions;
mod settings;
mod shortcut_editor;
#[cfg(all(feature = "integration-test", target_os = "linux"))]
pub(crate) use shortcut_editor::verify_native as verify_shortcut_editor;
mod state;
mod teleport;
mod version_mismatch;
mod whats_new;
mod workspace;
mod workspace_close;
mod worktree_note;
mod worktree_open;
mod worktree_render;
mod worktree_shortcut;
mod worktree_source;

#[cfg(test)]
mod announcement_tests;
#[cfg(test)]
mod font_size_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod version_mismatch_tests;
#[cfg(test)]
pub(crate) mod workspace_tests;
#[cfg(test)]
mod worktree_open_tests;
#[cfg(test)]
mod worktree_tests;

#[cfg(any(windows, test, feature = "integration-test"))]
pub(crate) use state::Cover;
pub(crate) use {
    colors::{accent, danger, online, teleported, tint},
    page::{Page, WorkspaceAction},
    state::{MenuState, Removal},
    version_mismatch::Notice as VersionNotice,
    worktree_shortcut::WorktreeShortcut,
    worktree_source::WorktreeSource,
};

pub(crate) use devices::{DeviceScope, SettingsHint, enter as enter_key};
use page::WorkspaceMenuAction;
use state::Submission;
use workspace::WorkspaceTarget;

/// A build without cloud providers never asks to create or destroy one.
#[cfg(not(feature = "cloud"))]
impl crate::HerdrWindow {
    fn cloud_batch_open(&self) -> bool {
        false
    }

    fn cloud_batch_page(&self, _: &mut gpui::Context<Self>) -> Option<gpui::Div> {
        None
    }
}

/// Shared trailing action geometry for pickers and workspace context menus.
fn action_icon(
    path: &'static str,
    selector: String,
    color: gpui::Rgba,
    hover_color: gpui::Rgba,
) -> gpui::Div {
    use gpui::{prelude::*, *};
    div()
        .group("menu-action-icon")
        .size(px(24.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::config::corners::CONTROL))
        .child(
            svg()
                .path(path)
                .size(px(14.))
                // GPUI skips SVG painting without an explicit color on the SVG;
                // the parent's text color is not inherited by SVG painting.
                .text_color(color)
                .group_hover("menu-action-icon", move |s| s.text_color(hover_color))
                .debug_selector(move || selector.clone()),
        )
}

/// Breathing room between a popup and the window's edges, so a list that had
/// to be clamped still shows that it stops short of the frame.
const MENU_MARGIN: f32 = 8.;

/// Daemon failures arrive as an open envelope. Keep the code so callers can
/// classify the refusal, and the message for display.
pub(crate) fn endpoint_error(error: &serde_json::Value) -> (&str, &str) {
    (
        error["code"].as_str().unwrap_or("endpoint_error"),
        error["message"].as_str().unwrap_or("Invalid daemon error"),
    )
}

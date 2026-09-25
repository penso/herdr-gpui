//! The window's menu popups: the pages they show, the state they own, and the
//! workspace operations they carry to the daemon. Input is isolated to the
//! popup while one is open, and every modal action is fenced by the connection
//! it was started under.

mod chrome;
mod colors;
mod devices;
mod git;
mod github;
mod page;
mod pr;
mod settings;
mod state;
mod workspace;
mod workspace_close;
mod worktree_open;
mod worktree_render;
mod worktree_source;

#[cfg(test)]
mod font_size_tests;
#[cfg(test)]
pub(crate) mod workspace_tests;
#[cfg(test)]
mod worktree_open_tests;
#[cfg(test)]
mod worktree_tests;

pub(crate) use {
    colors::accent,
    devices::{CONNECTED, Hint},
    page::{Page, WorkspaceAction},
    state::{MenuState, Removal},
    worktree_source::WorktreeSource,
};

use colors::danger;
use page::WorkspaceMenuAction;
use state::Submission;
use workspace::WorkspaceTarget;

/// Breathing room between a popup and the window's edges, so a list that had
/// to be clamped still shows that it stops short of the frame.
const MENU_MARGIN: f32 = 8.;

/// Daemon failures arrive as an open envelope. Keep the code so callers can
/// classify the refusal, and the message for display.
fn endpoint_error(error: &serde_json::Value) -> (&str, &str) {
    (
        error["code"].as_str().unwrap_or("endpoint_error"),
        error["message"].as_str().unwrap_or("Invalid daemon error"),
    )
}

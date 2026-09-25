//! The window's menu popups and dialogs: the pages they show, the state they
//! own, and the workspace operations they carry to the daemon. The kit draws
//! and focuses the surfaces; every modal action is fenced by the connection it
//! was started under.

mod chrome;
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
pub(crate) mod workspace_tests;
#[cfg(test)]
mod worktree_open_tests;

pub(crate) use {
    chrome::{dialog_buttons, error_alert, listener, submit},
    page::{Page, WorkspaceAction},
    state::{MenuState, Removal},
    worktree_render::bind_keys,
    worktree_source::WorktreeSource,
};

use page::WorkspaceMenuAction;
use state::Submission;
use workspace::WorkspaceTarget;

/// Daemon failures arrive as an open envelope. Keep the code so callers can
/// classify the refusal, and the message for display.
fn endpoint_error(error: &serde_json::Value) -> (&str, &str) {
    (
        error["code"].as_str().unwrap_or("endpoint_error"),
        error["message"].as_str().unwrap_or("Invalid daemon error"),
    )
}

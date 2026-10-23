//! Why a file did not open in the editor pane.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Another file is still opening in the editor")]
    Busy,
    #[error(
        "editor_command must be at most 1024 bytes without quotes, backslashes, or control characters"
    )]
    Command,
    #[error("No local pane to open the editor beside")]
    NoPane,
    #[error("This path cannot be typed into a shell safely, so it opens in the default app")]
    Path,
    #[error("The Neovim in the editor pane did not answer")]
    Remote,
    #[error("The Neovim in the editor pane is busy: answer it, then try again")]
    RemoteBusy,
    #[error("The Neovim in the editor pane could not open the file")]
    RemoteFailed,
    #[error("Could not run nvim to reach the editor pane")]
    RemoteLaunch(#[source] io::Error),
    #[error(transparent)]
    Request(std::sync::Arc<crate::Error>),
    #[error("Unexpected daemon response while opening the editor pane")]
    Response,
    #[error("Opening a terminal editor needs a Unix shell in the pane")]
    Unsupported,
    // keep-sorted end
}

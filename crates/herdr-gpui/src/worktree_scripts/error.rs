//! Why a worktree script could not be read or started.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Another worktree script is still starting")]
    Busy,
    #[error("The daemon did not identify this workspace's checkout")]
    Checkout,
    #[error(".herdr/worktree.toml is not valid UTF-8")]
    Encoding(#[source] std::str::Utf8Error),
    #[error("This workspace is not a Git checkout Herdr knows yet")]
    NotGit,
    #[error(".herdr/worktree.toml scripts must not contain NUL bytes")]
    Nul,
    #[error(".herdr/worktree.toml: {0}")]
    Parse(#[source] toml::de::Error),
    #[error("Could not read .herdr/worktree.toml: {0}")]
    Read(#[source] io::Error),
    #[error("Could not read .herdr/worktree.toml on the host: {0}")]
    Remote(#[source] herdr_client::Error),
    #[error("{0}")]
    Request(#[source] std::sync::Arc<crate::Error>),
    #[error("Unexpected daemon response while opening the script's tab")]
    Response,
    #[error(".herdr/worktree.toml exceeds {limit} bytes")]
    Size { limit: u64 },
    #[error("Worktree scripts cannot be read from a WSL distribution yet")]
    UnsupportedHost,
    // keep-sorted end
}

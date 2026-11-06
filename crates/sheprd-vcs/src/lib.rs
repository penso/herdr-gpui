//! Git on the checkouts Sheprd shows: working-tree status and the explicit
//! writes (commit, push, pull request creation, staging a hunk), checkpoints
//! of a checkout kept as Git refs on its own host, the file and symbol index
//! the code search reads, and the file tree and blame the code views show. Every operation runs a child process, so callers queue work
//! on these workers and never run it on the UI thread; no GPUI.

#![cfg_attr(test, allow(clippy::disallowed_methods))]

pub mod blame;
pub mod checkpoint;
pub mod code_index;
mod error;
pub mod file_tree;
pub mod git;
pub mod stage;

pub use error::{Error, Result};

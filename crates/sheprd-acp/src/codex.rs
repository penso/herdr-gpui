//! Codex without an ACP adapter. `codex app-server` speaks JSON-RPC on its
//! standard input and output, the protocol OpenAI's own editor extension
//! and Python SDK drive, and this module translates between it and the
//! session's ACP. OpenAI marks the app server experimental, so messages of
//! kinds this module does not know are skipped, and `herdr-acp codex --acp`
//! keeps the `codex-acp` adapter for when a release changes them.

mod approval;
mod background;
mod bridge;
mod items;
mod settings;

use std::ffi::OsString;

use crate::harness::{Reach, Translated};

/// The program this module starts.
pub(crate) const PROGRAM: &str = "codex";

/// The arguments that start Codex's app server.
pub(crate) fn args() -> Vec<OsString> {
    vec!["app-server".into()]
}

/// How the session reaches `codex app-server`.
pub(crate) fn reach() -> Box<dyn Reach> {
    Box::new(Translated(bridge::Bridge::default()))
}

#[cfg(test)]
mod tests;

//! pi without an ACP adapter. `pi --mode rpc` speaks JSON lines on its
//! standard input and output, documented in pi's `docs/rpc.md`, and this
//! module translates between them and the session's ACP. pi has no tool
//! approval of its own; an extension's questions become permission
//! requests.

mod bridge;
mod tools;

use std::ffi::OsString;

use crate::harness::{Reach, Translated};

/// The program this module starts.
pub(crate) const PROGRAM: &str = "pi";

/// The arguments that start pi's RPC mode, continuing session `load` when
/// set.
pub(crate) fn args(load: Option<&str>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--mode".into(), "rpc".into()];
    if let Some(session) = load {
        args.push("--session".into());
        args.push(session.into());
    }
    args
}

/// How the session reaches `pi --mode rpc`.
pub(crate) fn reach() -> Box<dyn Reach> {
    Box::new(Translated(bridge::Bridge::default()))
}

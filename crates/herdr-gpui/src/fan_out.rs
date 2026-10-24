//! Fan-out: send one prompt to several agents, each in a new worktree branched
//! from the same commit, then compare what they did and keep one.
//!
//! The GUI connection's method allowlist lacks `agent.start` and
//! `agent.prompt`, so, as Teleport does, every step runs as a background host
//! script calling the `herdr` CLI (locally or over SSH). Only existing daemon
//! methods are used: `worktree.create`, `agent.start`, `agent.prompt`, and
//! `worktree.remove`.
//!
//! Each lane's progress is journaled (see [`journal`]) so a fan-out cut short
//! by a quit or crash is shown again on the next launch, for the user to
//! finish or clean up.

mod error;
mod job;
mod journal;
mod plan;
mod spread;
mod state;
mod ui;

pub(crate) use journal::Journal;
pub(crate) use plan::MAX_LANES;
pub(crate) use state::{FanOut, Origin};

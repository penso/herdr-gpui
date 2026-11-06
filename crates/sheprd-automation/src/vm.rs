//! The sandboxed Luau state a script runs in. Its only effects are the
//! catalog's steps: no clock, randomness, files, network, processes,
//! `require`, or code loading, so a script makes the same calls every time.
//! The state itself is `herdr-luau`'s, under its deterministic policy.

use crate::Result;
use mlua::Lua;
use sheprd_luau::{Meter, Policy};

/// Bounds on one script's own computation; step work is bounded by caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Luau's interrupt checks: one per loop iteration and call, roughly.
    pub interrupts: u64,
    pub memory: usize,
    /// Steps one run may call.
    pub steps: usize,
}

impl Default for Limits {
    fn default() -> Self {
        let sheprd_luau::Limits { interrupts, memory } = sheprd_luau::Limits::default();
        Self {
            interrupts,
            memory,
            steps: 32,
        }
    }
}

impl From<Limits> for sheprd_luau::Limits {
    fn from(limits: Limits) -> Self {
        Self {
            interrupts: limits.interrupts,
            memory: limits.memory,
        }
    }
}

/// A state with the safe libraries and the budget, not yet sandboxed: the
/// caller adds the host's globals, then calls [`seal`].
pub(crate) fn open(limits: Limits, meter: &Meter) -> Result<Lua> {
    Ok(sheprd_luau::open(
        limits.into(),
        Policy::Deterministic,
        meter,
    )?)
}

/// Freezes every library and the host's globals.
pub(crate) fn seal(lua: &Lua) -> Result<()> {
    Ok(sheprd_luau::seal(lua)?)
}

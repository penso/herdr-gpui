//! The sandboxed Luau state Herdr runs user scripts in: only the safe
//! standard libraries, a memory limit, an interrupt budget, and globals
//! frozen once the host has added its own. Automations and mods differ in
//! what else they remove, which [`Policy`] names.
//!
//! ```
//! use herdr_luau::{Limits, Meter, Policy};
//!
//! let meter = Meter::default();
//! let lua = herdr_luau::open(Limits::default(), Policy::Interactive, &meter)?;
//! // Add the host's globals here, then freeze them.
//! herdr_luau::seal(&lua)?;
//! meter.reset();
//! herdr_luau::text(&lua, "assert(string.upper('a') == 'A')", "example").exec()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod error;
mod vm;

pub use error::{Error, OutOfInstructions, Result};
pub use vm::{Limits, Meter, Policy, open, seal, text};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

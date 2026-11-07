//! The push relay: forwards pushes to APNs for the iOS app, so a computer
//! needs no APNs key. `docs/apns-relay-plan.md` has the design.
//!
//! The relay should know as little as it can, so that taking it over gives
//! an attacker little:
//!
//! - Daemons hold [`Handle`]s, which the phone seals to the relay's key. The
//!   relay opens one in memory for each push and stores no push token.
//! - Payloads pass through, sealed by the daemon, and are neither stored nor
//!   logged. Nothing here may log a token, a handle, or a payload.
//! - The APNs topic and host come from the relay's own config, never from a
//!   caller.
//!
//! - [`credential`]: what proves a device may push, valid for thirty days.
//! - [`handle`]: a token and credential sealed to the relay.
//! - [`limit`]: rate limits, in memory.
//! - [`store`]: revocations, the only state kept, in memory unless the
//!   operator chooses a database.
//! - [`relay`]: the checks, in order, and the reply each one gives.
//! - [`web`]: the HTTP API.
//!
//! Issuing credentials waits on App Attest (`docs/apns-relay-plan.md`,
//! spike 3): nothing outside the tests calls [`credential::Issuer::issue`] yet.

pub mod credential;
mod error;
pub mod handle;
pub mod limit;
mod options;
pub mod relay;
mod serve;
pub mod store;
pub mod web;

pub use error::{Error, Result};
pub use handle::{Handle, Kind};
pub use options::{Backend, Options, USAGE};
pub use serve::serve;

#[cfg(test)]
mod tests;

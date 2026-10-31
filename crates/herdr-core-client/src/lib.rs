#![doc = include_str!("../README.md")]
// Tests start real processes; production code builds them with `new_command`.
#![cfg_attr(test, allow(clippy::disallowed_methods))]

mod backend;
mod connection;
mod endpoint;
mod error;
pub mod event;
pub mod frame;
pub mod hello;
mod method;
mod start;
mod stream;
pub mod wire;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use backend::{CoreBackend, CoreChanges, CoreTerminal};
pub use connection::Core;
pub use endpoint::Endpoint;
pub use error::{Error, Result, Violation};
pub use method::Method;
pub use start::{PROGRAM, start};
pub use stream::{Attachment, Chunk, Closed, End, MAX_BUFFERED, Subscription};

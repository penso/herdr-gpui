//! Fly Sprites, a `cloud` provider whose sprites become devices: persistent
//! Linux machines that sleep when idle and wake on the next command. A token
//! against the configured API root makes and deletes them over bounded REST
//! calls, and this app runs itself as a relay (see [`relay`]) that turns a
//! sprite's exec stream into the standard streams Herdr's bridge, installer,
//! and Teleport's scripts already use. Everything here blocks, so it runs on
//! background workers.

mod api;
mod connect;
mod error;
mod http;
pub mod relay;
mod settings;
pub mod setup;
mod store;

pub use error::{Error, Status};
pub use {
    connect::{command, connect},
    settings::{DEFAULT_API_URL, Settings},
    setup::destroy_saved,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;

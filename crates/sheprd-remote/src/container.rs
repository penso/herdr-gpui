//! Containers on this computer's Docker or Podman engine, as cloud devices:
//! each one a private Linux machine with its own Herdr daemon, home volume,
//! and resource limits, so agents in it cannot see this computer's files or
//! each other's. Everything goes through the engine's own CLI, which already
//! follows Docker Desktop, OrbStack, colima, and `podman machine` contexts.
//!
//! Every call here blocks on the engine, so it runs on background workers.

mod connect;
mod engine;
mod error;
pub mod idle;
pub mod image;
pub mod lifecycle;
mod machine;
mod settings;
pub mod setup;

pub use error::Error;
pub use {
    connect::{command, connect},
    engine::Engine,
    settings::Settings,
    setup::destroy_saved,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;

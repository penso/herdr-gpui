//! The machines Sheprd reaches beyond this computer's own daemon: the cloud
//! providers that create them (each behind its feature), running scripts on
//! any host, and Teleport, which moves a session's work to another host. No
//! GPUI: the app runs these on its own background workers and shows their
//! progress and dialogs.
// Tests start fixture processes directly; production children use
// sheprd_client::new_command.
#![cfg_attr(test, allow(clippy::disallowed_methods))]

#[cfg(feature = "azure")]
pub mod azure;
#[cfg(feature = "boat")]
pub mod boat;
#[cfg(feature = "cloud")]
pub mod cloud;
#[cfg(feature = "coder")]
pub mod coder;
pub mod config;
#[cfg(feature = "container")]
pub mod container;
#[cfg(any(
    feature = "azure",
    feature = "boat",
    feature = "coder",
    feature = "daytona",
    feature = "opensandbox",
    feature = "sprites"
))]
mod credentials;
#[cfg(feature = "daytona")]
pub mod daytona;
mod error;
#[cfg(feature = "opensandbox")]
pub mod opensandbox;
#[cfg(feature = "railway")]
pub mod railway;
#[cfg(feature = "sprites")]
pub mod sprites;
pub mod teleport;

pub use error::{Error, Result};

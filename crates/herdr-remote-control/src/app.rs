//! Herdr UI's side of remote control, without the server: the `[remote]`
//! table, the daemon as a user service, and the pairing dialog's state and
//! QR code. Nothing here needs Tokio or GPUI; the app runs the blocking
//! parts off its UI thread and draws the rest.

pub mod config;
mod error;
pub mod pairing;
pub mod qr;
pub mod service;

pub use error::{Error, Result};

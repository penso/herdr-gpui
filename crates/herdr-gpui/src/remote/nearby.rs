//! Nearby pairing over Bluetooth, Herdr UI's side (`docs/ios-app-plan.md`,
//! "Bluetooth on the desktop"). While the pairing dialog shows an offer,
//! `herdr-nearby`, the helper beside the executable (Swift on macOS, Rust
//! with BlueZ or WinRT on Linux and Windows, `crates/herdr-nearby`),
//! advertises the nearby service and hands over each phone's `pair` frame;
//! the dialog relays it to the daemon's control socket and writes the
//! answer back. The helper holds no secrets and the code exchange runs in
//! the daemon: this only carries bytes. Without the helper (development
//! builds that did not build it) nothing starts, and the network and QR
//! paths remain.
//!
//! The helper speaks newline-delimited JSON on stdio (its header comment in
//! `macos/herdr-nearby/main.swift` lists the messages).
//!
//! With `[remote] presence` on, the helper also runs while remote control
//! does, advertising for paired phones' presence ([`presence`]). One
//! [`Station`] owns the one helper for both.

// keep-sorted start
mod error;
pub(crate) mod presence;
mod relay;
mod station;
// keep-sorted end

pub use error::Error;
pub(crate) use relay::*;
pub(crate) use station::{Advert, Station};

/// The computer's Bluetooth radio, as the helper last reported it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Radio {
    On,
    Off,
    /// Herdr is not allowed to use Bluetooth (macOS: System Settings >
    /// Privacy).
    Unauthorized,
    Unsupported,
    /// Resetting or not known yet.
    Unknown,
}

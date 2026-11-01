//! The platform's Bluetooth peripheral: BlueZ over D-Bus on Linux, WinRT's
//! GATT server on Windows, and on anything else a report that Bluetooth is
//! unsupported, after which the helper only reads its input until it ends.

use crate::{
    Result,
    peripheral::{self, Backend, Shared},
    protocol::{Channel, Command, Out},
};
use std::sync::mpsc::Receiver;

/// The nearby service and its characteristics, as the phone looks for them.
#[cfg(any(target_os = "linux", windows))]
pub(crate) const SERVICE: &str = "dd750b9a-9568-4fc3-b357-b7515ae46ded";
/// Read: `{"v":1,"name":…,"fp":…}` while pairing, `{"v":1}` for presence.
#[cfg(any(target_os = "linux", windows))]
pub(crate) const INFO: &str = "820a3154-f1b2-47c2-b8be-d89e812464a2";
/// Write and notify: framed pairing messages.
#[cfg(any(target_os = "linux", windows))]
pub(crate) const PAIR: &str = "8d5498fd-cfe2-4d2c-99e9-1fdbc2d83892";
/// Write and notify: paired phones' presence, framed like `pair`.
#[cfg(any(target_os = "linux", windows))]
pub(crate) const PRESENCE: &str = "6e3a4f21-8c0b-4d7e-9a55-2f1c0b7d4e93";

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

/// Runs until Herdr UI's input ends.
pub(crate) fn run(
    bluetooth: bool,
    core: &Shared,
    commands: &Receiver<Command>,
    out: &Out,
) -> Result<()> {
    if !bluetooth {
        peripheral::serve(core, &Off, commands, out);
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    let started = linux::Linux::start(core.clone(), out.clone());
    #[cfg(windows)]
    let started = windows::Windows::start(core.clone(), out.clone());
    #[cfg(any(target_os = "linux", windows))]
    {
        match started {
            Ok(backend) => peripheral::serve(core, &backend, commands, out),
            // Herdr UI hears why, and the radio as unavailable; the
            // commands are still read so it never blocks writing them.
            Err(error) => {
                out.emit(&crate::protocol::Event::error(&error));
                peripheral::serve(core, &Off, commands, out);
            }
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        out.emit(&crate::protocol::Event::State {
            state: crate::protocol::Radio::Unsupported,
        });
        peripheral::serve(core, &Off, commands, out);
        Ok(())
    }
}

/// No radio: commands still change what would be advertised, so `info`
/// and the idle reports behave, but nothing reaches a phone.
struct Off;

impl Backend for Off {
    fn refresh(&self, _advertise: bool) {}

    fn notify(&self, _device: &str, _channel: Channel, _chunks: Vec<Vec<u8>>) {}

    fn idle_seconds(&self) -> Option<u64> {
        None
    }
}

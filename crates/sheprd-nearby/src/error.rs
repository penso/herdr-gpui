/// Why the helper's Bluetooth side could not run.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[cfg(target_os = "linux")]
    #[error("BlueZ refused: {0}")]
    Bluez(#[from] zbus::fdo::Error),
    #[cfg(target_os = "linux")]
    #[error("could not reach BlueZ over D-Bus")]
    Bus(#[from] zbus::Error),
    #[error("Bluetooth refused to {step}: error {code}")]
    Gatt { step: &'static str, code: i32 },
    #[cfg(windows)]
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

//! Remote control: the daemon's status and devices, the pairing dialog, and
//! the revoke confirmation. The daemon's own types are reused as they are;
//! no pairing token ever crosses this socket.
use herdr_remote_control::control::{Device, Status};
use serde::{Deserialize, Serialize};

/// Asking the daemon, and for `revoke` the device list first, takes up to
/// its own answer timeout a request.
const DAEMON_WAIT_SECONDS: u64 = 12;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteOp {
    Status,
    /// Opens the pairing dialog in the window.
    Pair,
    Devices,
    /// Opens the confirmation for revoking a device, by ID or name.
    Revoke {
        device: String,
    },
}

impl RemoteOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Status | Self::Devices | Self::Revoke { .. } => DAEMON_WAIT_SECONDS,
            Self::Pair => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Status {
        /// Whether remote control is on in Settings.
        enabled: bool,
        /// What the daemon says, when it answers.
        #[serde(default)]
        status: Option<Box<Status>>,
        /// Why it did not, when it is on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Devices {
        devices: Vec<Device>,
    },
}

#[cfg(test)]
mod tests;

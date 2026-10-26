//! The wire between Herdr UI and the daemon: one request and one response
//! per connection, as length-prefixed JSON frames over a private endpoint
//! (`herdr_client::private_endpoint`) in the daemon's state directory, so
//! only this user reaches it. No Tokio: Herdr UI calls [`request`] from a
//! background thread.
//!
//! Secrets never cross it except a pairing offer's URL, whose token is
//! valid for one minute and one claim. Device keys and session tokens stay
//! in the daemon.

use herdr_chat_protocol::{read_frame, write_frame};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::Duration,
};

/// How long a request waits for a pipe instance on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a client waits for the daemon's answer.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("could not reach the remote control daemon")]
    Connect(#[source] std::io::Error),
    #[error("the remote control daemon refused: {message}")]
    Daemon { code: ErrorCode, message: String },
    #[error(transparent)]
    Frame(#[from] herdr_chat_protocol::Error),
    #[error("the remote control daemon closed the connection without answering")]
    NoAnswer,
    // keep-sorted end
}

/// Where the daemon whose state lives in `state_dir` listens for Herdr UI.
pub fn address(state_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        // A pipe name, unique per state directory.
        let digest = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            state_dir.to_string_lossy().as_bytes(),
        );
        return PathBuf::from(format!(r"\\.\pipe\herdr-remote-{}", digest.simple()));
    }
    state_dir.join("control.sock")
}

/// Removes what a daemon's listener left in `state_dir`.
pub fn release(state_dir: &Path) {
    herdr_client::private_endpoint::release(&address(state_dir));
}

/// Sends one request and waits for its response. Blocking.
pub fn request(address: &Path, request: &Request) -> Result<Response, Error> {
    let connection = herdr_client::private_endpoint::Connection::connect(address, CONNECT_TIMEOUT)
        .map_err(Error::Connect)?;
    let (mut reader, writer, closer) = connection.split().map_err(Error::Connect)?;
    reader
        .set_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(Error::Connect)?;
    let mut writer = BufWriter::new(writer);
    write_frame(&mut writer, request)?;
    writer.flush().map_err(Error::Connect)?;
    let answer = read_frame::<_, Response>(&mut BufReader::new(reader));
    closer.close();
    match answer?.ok_or(Error::NoAnswer)? {
        Response::Error { code, message } => Err(Error::Daemon { code, message }),
        response => Ok(response),
    }
}

/// A pairing in progress, named by the daemon.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PairingId(pub String);

/// A paired device, named by the daemon.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "request")]
pub enum Request {
    /// Whether the daemon runs, where it listens, and who is connected.
    Status,
    /// Starts a pairing, or rotates the token of the open one: the QR the
    /// dialog shows. Each call voids the previous token.
    Pair,
    /// Where the pairing stands; the dialog polls it.
    Pairing {
        id: PairingId,
    },
    /// The user's answer to a claimed pairing in the desktop dialog, with
    /// the matching code it showed: an approval for any other claim is
    /// refused.
    Approve {
        id: PairingId,
        allow: bool,
        code: String,
    },
    /// Closes the dialog's pairing; its token stops working.
    CancelPairing {
        id: PairingId,
    },
    Devices,
    /// Revokes a device: its sessions end and its push subscription is
    /// dropped at once. Herdr UI asks the user before sending it.
    Revoke {
        device: DeviceId,
    },
    /// Stops the daemon. The service manager does not restart it.
    Shutdown,
    /// The status marks the user picked in Herdr UI, by its config name
    /// (`ui.status_icons`), for the phone's lists to draw the same. A set
    /// the phone cannot draw shows as dots.
    Indicators {
        set: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "response")]
pub enum Response {
    Status(Status),
    Offer(Offer),
    Pairing(PairingState),
    Devices {
        devices: Vec<Device>,
    },
    /// The request was carried out.
    Done,
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// No such pairing, or it expired.
    UnknownPairing,
    /// No such device.
    UnknownDevice,
    /// The pairing is not waiting for an answer (not claimed yet, or done).
    NotClaimed,
    /// The request does not fit this daemon's version.
    Unsupported,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The daemon's version, for Herdr UI to restart an older one.
    pub version: String,
    /// The port HTTPS listens on.
    pub port: u16,
    /// Every `https://` origin the certificate covers, LAN first.
    pub addresses: Vec<String>,
    /// SHA-256 of the certificate authority, colon-separated hex.
    pub fingerprint: String,
    /// Whether the daemon reaches a Herdr daemon on this machine.
    pub herdr: bool,
    pub chats: usize,
    pub devices: usize,
    /// Devices with an open stream now.
    pub connected: usize,
}

/// What the pairing dialog shows: a QR of `url`, valid until `expires_in_ms`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub id: PairingId,
    /// `https://ADDRESS:PORT/pair#t=TOKEN&fp=FINGERPRINT`.
    pub url: String,
    pub fingerprint: String,
    pub expires_in_ms: u64,
    /// Every origin the phone may reach this computer at.
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum PairingState {
    /// No phone has claimed the token yet.
    Waiting {
        expires_in_ms: u64,
    },
    /// A phone claimed it: the dialog shows its name and the matching code
    /// and asks the user.
    Claimed {
        device_name: String,
        code: String,
        /// The origin of the Home Screen app that claimed it, when that app
        /// was installed from another computer: approving admits it to this
        /// computer's API, so the dialog names it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
    },
    Approved {
        device: DeviceId,
    },
    Denied,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: u64,
    pub last_seen_ms: Option<u64>,
    /// Whether it receives push notifications.
    pub push: bool,
    /// Whether it has a stream open now.
    pub connected: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

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
pub mod hosts;
pub mod pull_requests;
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
        /// The hosts ticked in the dialog, for the phone to be paired
        /// with too (`hosts`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hosts: Vec<hosts::HostId>,
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
    /// The desktop's cached PR badges, fenced by daemon boot and workspace identity.
    PullRequests {
        snapshot: pull_requests::PullRequests,
    },
    /// A phone nearby starts pairing over Bluetooth, which Herdr UI relays:
    /// the first step of the code exchange (`docs/ios-app-plan.md`,
    /// "Nearby pairing, exactly"). Refused unless a pairing dialog is open.
    NearbyStart {
        /// The phone's one-time P-256 key, SEC1 uncompressed, base64url.
        public_key: String,
    },
    /// The second step: the phone's nonce, answered with the daemon's and
    /// the sealed pairing token.
    NearbyReveal {
        session: String,
        nonce: String,
    },
    /// A paired phone near over Bluetooth says hello with its daily hint,
    /// which Herdr UI relays: answered with a nonce, signed by this
    /// computer's CA (`docs/ios-app-plan.md`, "Presence, exactly").
    PresenceChallenge {
        hint: String,
    },
    /// The phone's signature over `herdr-presence-v1 || CA fingerprint ||
    /// nonce`; the nonce names the device it was given for.
    PresenceProve {
        nonce: String,
        signature: String,
    },
    /// Whether a proven phone's owner is at this computer: while near, the
    /// device's alerts are held. A mark not renewed lapses after two
    /// minutes.
    PresenceSet {
        device: DeviceId,
        near: bool,
    },
    /// The SSH and cloud hosts Herdr UI connects to, which a phone paired
    /// here may be paired with too. Replaces the previous list.
    Hosts {
        hosts: Vec<hosts::Host>,
    },
    /// Takes the phones' requests to be paired with hosts that Herdr UI
    /// has not taken yet.
    HostPairings,
    /// How Herdr UI's attempt to pair `device` with `host` ended.
    HostPaired {
        device: DeviceId,
        host: hosts::HostId,
        /// The key of the request it took (`HostPairing::public_key`), so
        /// an attempt that outlasted a retry cannot answer the retry.
        /// Absent from an older Herdr UI, which matches any request.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        public_key: Option<String>,
        outcome: hosts::HostOutcome,
    },
    /// The devices revoked here that hosts still hold.
    HostRevokes,
    /// A host no longer holds the device.
    HostRevoked(hosts::HostRevoke),
    /// On a host: adds a phone another computer's user allowed, as
    /// `herdr-remote-control enroll` asks. Herdr UI never sends it to its
    /// own daemon.
    Enroll(hosts::Enroll),
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
    NearbyStarted(NearbyStarted),
    NearbyRevealed(NearbyRevealed),
    /// The nonce a presence challenge asks the phone to sign, with this
    /// computer's CA certificate (DER) and its signature over
    /// `herdr-presence-challenge-v1 || nonce || device id`, all base64url.
    PresenceChallenge {
        certificate: String,
        nonce: String,
        signature: String,
    },
    /// The phone proved it is `device`. `addresses` is the computer's
    /// current origins sealed to the device's notify key, for the phone to
    /// find it again when its address changed; `None` when the device
    /// registered no key.
    PresenceProven {
        device: DeviceId,
        addresses: Option<String>,
    },
    HostPairings {
        pairings: Vec<hosts::HostPairing>,
    },
    HostRevokes {
        revokes: Vec<hosts::HostRevoke>,
    },
    Enrolled(hosts::Enrolled),
}

/// The daemon's answer to a nearby phone's first step, the same JSON over
/// `POST /api/nearby/start` and the control socket.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NearbyStarted {
    pub session: String,
    /// The name the phone shows for this computer.
    pub computer: String,
    /// SHA-256 of the certificate authority, 64 uppercase hex digits.
    pub fingerprint: String,
    /// The daemon's one-time P-256 key, SEC1 uncompressed, base64url.
    pub public_key: String,
    /// SHA-256 of the domain, both keys, and the daemon's nonce, base64url:
    /// the daemon cannot change its nonce after seeing the phone's.
    pub commit: String,
}

/// The daemon's answer to the second step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NearbyRevealed {
    /// The daemon's nonce, base64url.
    pub nonce: String,
    /// AES-256-GCM nonce then ciphertext of `{"token", "origins"}`, base64url.
    pub sealed: String,
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
    /// Nearby pairing needs the pairing dialog open on the computer.
    NoPairing,
    /// The daemon could not write its state; nothing changed, and the
    /// same request may succeed later.
    Unsaved,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The local Herdr session whose workspaces this daemon exposes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub boot_id: String,
    /// Interface metadata keyed by advertised origin; absent on older daemons.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub address_details: std::collections::BTreeMap<String, AddressDetails>,
    /// The daemon's version, for Herdr UI to restart an older one.
    pub version: String,
    /// The port HTTPS listens on.
    pub port: u16,
    /// Every `https://` origin the certificate covers, Tailscale first, then LAN.
    pub addresses: Vec<String>,
    /// SHA-256 of the certificate authority, colon-separated hex.
    pub fingerprint: String,
    /// The name this computer gives itself, which phones show.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub computer: String,
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
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub address_details: std::collections::BTreeMap<String, AddressDetails>,
    pub id: PairingId,
    /// `https://ADDRESS:PORT/pair#t=TOKEN&fp=FINGERPRINT`.
    pub url: String,
    pub fingerprint: String,
    pub expires_in_ms: u64,
    /// Every origin the phone may reach this computer at.
    pub addresses: Vec<String>,
}

/// Observed on the daemon's host, never inferred from the GUI host's adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddressDetails {
    pub interfaces: Vec<String>,
    pub kind: AddressKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressKind {
    // keep-sorted start
    Bridge,
    Local,
    Loopback,
    Tailscale,
    // keep-sorted end
    // Serde's forward-compatible fallback must be last.
    #[default]
    #[serde(other)]
    Other,
}

impl AddressKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bridge => "Network bridge",
            Self::Local => "Local network",
            Self::Loopback => "This computer only",
            Self::Other => "Network address",
            Self::Tailscale => "Tailscale",
        }
    }
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
    /// The computer that vouched for it, when another computer's user
    /// allowed it (`hosts`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// The other hosts it was paired with from here, or may be.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<hosts::HostLink>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

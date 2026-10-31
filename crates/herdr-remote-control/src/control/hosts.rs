//! Pairing a phone with the other hosts Herdr UI connects to (SSH and cloud
//! machines), so the phone reaches each host's own daemon directly and keeps
//! them while this computer sleeps.
//!
//! This daemon only relays. Herdr UI tells it which hosts it knows
//! ([`Host`]) and which ones the user ticked when allowing a phone. The
//! phone sends one fresh key per host over the authenticated API. Herdr UI
//! takes each request ([`HostPairing`]), enrolls the key on the host over
//! SSH ([`Enroll`] on that host's daemon), and reports the outcome back
//! ([`HostOutcome`]), which the phone reads with the host's fingerprint
//! and addresses.

use super::DeviceId;
use serde::{Deserialize, Serialize};

/// The most hosts Herdr UI may name.
pub const MAX_HOSTS: usize = 256;
/// The longest host id or name accepted.
pub const MAX_HOST_TEXT: usize = 256;
/// The longest failure reason kept for a phone to show.
pub const MAX_REASON: usize = 512;
/// The most origins one enrolled host may report.
pub const MAX_ORIGINS: usize = 32;

/// A host, named by Herdr UI's endpoint id (`ssh:PROFILE`, `coder:ID`).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostId(pub String);

/// A host Herdr UI connects to, with the name it shows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Host {
    pub id: HostId,
    pub name: String,
}

impl Host {
    pub fn valid(&self) -> bool {
        text(&self.id.0) && text(&self.name)
    }
}

fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_HOST_TEXT && !value.chars().any(char::is_control)
}

/// Where one phone stands with one host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum HostState {
    /// Known to Herdr UI, not allowed for this phone: pairing it asks the
    /// desktop first.
    Offered,
    /// Allowed on the desktop; waiting for the phone's key.
    Allowed,
    /// The phone asked for a host it was not allowed; the desktop has not
    /// answered.
    Asking,
    /// Herdr UI is pairing it.
    Pending,
    /// The host's daemon holds the phone's key.
    Paired(Enrolled),
    /// The last attempt failed; pairing again retries.
    Failed { reason: String },
}

/// One host as a phone sees it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostLink {
    pub host: HostId,
    pub name: String,
    #[serde(flatten)]
    pub state: HostState,
}

/// A phone's request to be paired with a host, for Herdr UI to carry out.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostPairing {
    /// The phone, as this computer knows it.
    pub device: DeviceId,
    pub host: HostId,
    /// The phone's name, kept on the host.
    pub name: String,
    /// The key the phone made for this host, SEC1 uncompressed, base64url.
    pub public_key: String,
    /// The desktop did not allow this host when it allowed the phone, so
    /// Herdr UI asks the user before pairing it.
    pub confirm: bool,
    /// The phone's earlier device on that host, which the new one replaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<DeviceId>,
}

/// Asks a host's daemon to add a phone the user allowed on another
/// computer. Sent over the host's own control socket by
/// `herdr-remote-control enroll`, which Herdr UI runs over SSH.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Enroll {
    pub name: String,
    pub public_key: String,
    /// The computer that vouched for the phone, shown beside the device.
    pub via: String,
    /// A device this one replaces: the same phone's earlier key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<DeviceId>,
}

/// A host's answer to [`Enroll`]: what the phone needs to reach it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Enrolled {
    /// The phone's id on that host.
    pub device: DeviceId,
    /// The name the host calls itself.
    pub computer: String,
    /// SHA-256 of the host's certificate authority, colon-separated hex.
    pub fingerprint: String,
    /// Every `https://` origin the host answers at, Tailscale first.
    pub origins: Vec<String>,
}

impl Enrolled {
    pub fn valid(&self) -> bool {
        text(&self.device.0)
            && text(&self.computer)
            && text(&self.fingerprint)
            && self.origins.len() <= MAX_ORIGINS
            && self.origins.iter().all(|origin| text(origin))
    }
}

/// How Herdr UI's attempt to pair a phone with a host ended.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum HostOutcome {
    Paired(Enrolled),
    Failed {
        reason: String,
    },
    /// The user declined a host the phone asked for later.
    Denied,
}

/// A phone's device on a host, still to be revoked there because the
/// phone was revoked here.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct HostRevoke {
    pub host: HostId,
    pub device: DeviceId,
}

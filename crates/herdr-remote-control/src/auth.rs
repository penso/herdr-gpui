//! Pairing phones and authenticating their requests, as the plan's
//! "Pairing and authentication" describes: a short-lived single-use token in
//! a QR, a matching code a person approves on the desktop, a device key that
//! signs a challenge for every session, and bearer tokens kept only as
//! digests.
//!
//! Plain synchronous state: the server holds an [`Auth`] behind a
//! `std::sync::Mutex` and passes every time explicitly, in milliseconds
//! since the Unix epoch, so the rules are tested without clocks or sockets.

mod devices;
mod handoff;
mod limits;
mod pairing;
mod session;

pub(crate) use devices::Devices;
pub(crate) use handoff::{Handoff, Handoffs};
pub(crate) use limits::RateLimiter;
pub(crate) use pairing::{Claim, ClaimId, NewDevice, Pairings};
pub(crate) use session::{Challenge, ChallengeId, Session, Sessions};

use crate::control::{self, DeviceId, Offer, PairingId, PairingState};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::VerifyingKey;
use sha2::{Digest, Sha256};
use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock},
};
use tokio::sync::watch;

/// Length of a SEC1 uncompressed P-256 point, as WebCrypto's
/// `exportKey("raw")` produces.
const PUBLIC_KEY_LEN: usize = 65;
/// Length of every random secret: tokens and nonces.
const SECRET_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("this pairing token was already claimed")]
    AlreadyClaimed,
    #[error("the approval names another phone's code; pair again")]
    CodeMismatch,
    #[error("could not encode the device list")]
    Encode(#[source] serde_json::Error),
    #[error("this pairing token expired")]
    ExpiredToken,
    #[error("the device key is not a P-256 public key")]
    InvalidKey,
    #[error("a device name must be 1 to 64 characters with no control characters")]
    InvalidName,
    #[error("the signature does not match the challenge")]
    InvalidSignature,
    #[error("the pairing token is not valid")]
    InvalidToken,
    #[error("the remote control daemon has no address to offer")]
    NoAddress,
    #[error("the pairing is not waiting for an answer")]
    NotClaimed,
    #[error("could not draw random bytes")]
    Random(#[source] getrandom::Error),
    #[error("too many requests; try again shortly")]
    RateLimited,
    #[error("could not {operation} {}", path.display())]
    Store {
        operation: StoreOperation,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a valid device list", path.display())]
    StoreFormat {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("no such challenge, or it expired")]
    UnknownChallenge,
    #[error("no such claim, or it is forgotten")]
    UnknownClaim,
    #[error("no such device")]
    UnknownDevice,
    #[error("no such pairing, or it expired")]
    UnknownPairing,
    // keep-sorted end
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The file operation a [`Error::Store`] failed at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreOperation {
    CreateDirectory,
    CreateTemporary,
    Read,
    Replace,
    Sync,
    Write,
}

impl std::fmt::Display for StoreOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CreateDirectory => "create the directory of",
            Self::CreateTemporary => "create a temporary file beside",
            Self::Read => "read",
            Self::Replace => "replace",
            Self::Sync => "sync",
            Self::Write => "write",
        })
    }
}

impl From<&Error> for control::ErrorCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::UnknownPairing => Self::UnknownPairing,
            Error::UnknownDevice => Self::UnknownDevice,
            Error::NotClaimed => Self::NotClaimed,
            _ => Self::Other,
        }
    }
}

/// A device's public key: a P-256 point checked to lie on the curve, kept
/// as the SEC1 uncompressed bytes WebCrypto exports and stored as base64url.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct PublicKey([u8; PUBLIC_KEY_LEN]);

impl PublicKey {
    /// Reads base64url (no padding) of a SEC1 uncompressed point.
    pub(crate) fn parse(encoded: &str) -> Result<Self> {
        let bytes: [u8; PUBLIC_KEY_LEN] =
            decode_fixed(encoded).map_err(|DecodeError| Error::InvalidKey)?;
        if bytes[0] != 0x04 {
            return Err(Error::InvalidKey);
        }
        VerifyingKey::from_sec1_bytes(&bytes).map_err(|_| Error::InvalidKey)?;
        Ok(Self(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn verifying_key(&self) -> Result<VerifyingKey> {
        VerifyingKey::from_sec1_bytes(&self.0).map_err(|_| Error::InvalidKey)
    }
}

impl TryFrom<String> for PublicKey {
    type Error = Error;

    fn try_from(encoded: String) -> Result<Self> {
        Self::parse(&encoded)
    }
}

impl From<PublicKey> for String {
    fn from(key: PublicKey) -> Self {
        encode(&key.0)
    }
}

/// Checks a device name and returns it without surrounding whitespace.
pub(crate) fn device_name(name: &str) -> Result<String> {
    let name = name.trim();
    let length = name.chars().count();
    if !(1..=64).contains(&length) || name.chars().any(char::is_control) {
        return Err(Error::InvalidName);
    }
    Ok(name.to_owned())
}

/// Input that is not base64url of the expected length.
#[derive(Debug)]
pub(crate) struct DecodeError;

/// Decodes base64url (no padding) into exactly `N` bytes, refusing longer
/// input before decoding so a request cannot make it allocate.
pub(crate) fn decode_fixed<const N: usize>(
    encoded: &str,
) -> std::result::Result<[u8; N], DecodeError> {
    if encoded.len() != base64::encoded_len(N, false).ok_or(DecodeError)? {
        return Err(DecodeError);
    }
    let mut bytes = [0; N];
    match URL_SAFE_NO_PAD.decode_slice(encoded, &mut bytes) {
        Ok(written) if written == N => Ok(bytes),
        _ => Err(DecodeError),
    }
}

pub(crate) fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(Error::Random)?;
    Ok(bytes)
}

/// A random identifier for pairings, claims, challenges, and devices.
pub(crate) fn random_id() -> Result<String> {
    Ok(encode(&random_bytes::<16>()?))
}

pub(crate) fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}

/// Compares two secrets in time that depends only on their lengths.
pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let difference = left
        .iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right));
    std::hint::black_box(difference) == 0
}

/// Rate limits the server applies per client address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Limits {
    pub(crate) pair_burst: u32,
    pub(crate) pair_refill_ms: u64,
    pub(crate) challenge_burst: u32,
    pub(crate) challenge_refill_ms: u64,
    /// Addresses each limiter remembers.
    pub(crate) addresses: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            addresses: 1024,
            challenge_burst: 20,
            challenge_refill_ms: 3_000,
            pair_burst: 5,
            pair_refill_ms: 12_000,
        }
    }
}

/// Everything the server needs to pair and authenticate phones.
#[derive(Debug)]
pub(crate) struct Auth {
    /// `Devices::apps`, kept current for CORS, which must not wait on this
    /// state's lock.
    apps: Arc<RwLock<Vec<String>>>,
    challenge_limit: RateLimiter<IpAddr>,
    devices: Devices,
    handoffs: Handoffs,
    pair_limit: RateLimiter<IpAddr>,
    pairings: Pairings,
    /// Bumped whenever sessions end early, so open streams check again.
    revoked: watch::Sender<u64>,
    sessions: Sessions,
}

impl Auth {
    /// Loads the paired devices from `state_dir`. Blocking.
    pub(crate) fn load(state_dir: &Path, limits: Limits) -> Result<Self> {
        let devices = Devices::load(state_dir)?;
        Ok(Self {
            apps: Arc::new(RwLock::new(devices.apps())),
            challenge_limit: RateLimiter::new(
                limits.challenge_burst,
                limits.challenge_refill_ms,
                limits.addresses,
            ),
            devices,
            handoffs: Handoffs::load(state_dir),
            pair_limit: RateLimiter::new(
                limits.pair_burst,
                limits.pair_refill_ms,
                limits.addresses,
            ),
            pairings: Pairings::default(),
            revoked: watch::Sender::new(0),
            sessions: Sessions::default(),
        })
    }

    /// Changes whenever a device loses access before its sessions expire.
    pub(crate) fn watch_revocations(&self) -> watch::Receiver<u64> {
        self.revoked.subscribe()
    }

    fn revoked(&self) {
        self.revoked.send_modify(|revision| *revision += 1);
    }

    /// The origins of apps installed from other computers that paired
    /// devices use, shared with the server's CORS check.
    pub(crate) fn apps(&self) -> Arc<RwLock<Vec<String>>> {
        self.apps.clone()
    }

    fn sync_apps(&self) {
        *self.apps.write().unwrap_or_else(PoisonError::into_inner) = self.devices.apps();
    }

    pub(crate) fn devices(&self) -> &Devices {
        &self.devices
    }

    pub(crate) fn devices_mut(&mut self) -> &mut Devices {
        &mut self.devices
    }

    pub(crate) fn mint(
        &mut self,
        now_ms: u64,
        origins: &[String],
        fingerprint: &str,
    ) -> Result<Offer> {
        self.pairings.mint(now_ms, origins, fingerprint)
    }

    pub(crate) fn pairing(&mut self, id: &PairingId, now_ms: u64) -> Result<PairingState> {
        self.pairings.state(id, now_ms)
    }

    pub(crate) fn cancel(&mut self, id: &PairingId) -> Result<()> {
        self.pairings.cancel(id)
    }

    /// A phone at `client` claims a pairing token.
    pub(crate) fn claim(
        &mut self,
        client: IpAddr,
        token: &str,
        device_name: &str,
        public_key: &str,
        now_ms: u64,
    ) -> Result<Claim> {
        if !self.pair_limit.allow(client, now_ms) {
            return Err(Error::RateLimited);
        }
        self.pairings.claim(token, device_name, public_key, now_ms)
    }

    /// Notes that `claim` came from an app installed from another computer.
    pub(crate) fn claimed_from(&mut self, claim: &ClaimId, origin: String) {
        self.pairings.claimed_from(claim, origin);
    }

    pub(crate) fn claim_status(&mut self, id: &ClaimId, now_ms: u64) -> Result<PairingState> {
        self.pairings.claim_status(id, now_ms)
    }

    /// The desktop's answer for the claim whose matching code it showed; an
    /// approval stores the device before the phone can learn it was
    /// approved. An approval naming any other code denies the pairing.
    pub(crate) fn answer(
        &mut self,
        id: &PairingId,
        allow: bool,
        shown: &str,
        now_ms: u64,
    ) -> Result<PairingState> {
        if allow {
            let matches = matches!(
                self.pairings.state(id, now_ms)?,
                PairingState::Claimed { code, .. } if constant_time_eq(code.as_bytes(), shown.as_bytes())
            );
            if !matches {
                self.pairings.answer(id, false, now_ms, |_| Ok(()))?;
                return Err(Error::CodeMismatch);
            }
        }
        let devices = &mut self.devices;
        self.pairings.answer(id, allow, now_ms, |device| {
            devices.add(device.clone(), now_ms)
        })?;
        self.sync_apps();
        self.pairings.state(id, now_ms)
    }

    pub(crate) fn challenge(
        &mut self,
        client: IpAddr,
        device: &DeviceId,
        now_ms: u64,
    ) -> Result<Challenge> {
        if !self.challenge_limit.allow(client, now_ms) {
            return Err(Error::RateLimited);
        }
        self.sessions.challenge(device, now_ms, &self.devices)
    }

    /// Checks a signed challenge, starts a session, and marks the device seen.
    pub(crate) fn verify(
        &mut self,
        challenge: &ChallengeId,
        signature: &str,
        now_ms: u64,
    ) -> Result<Session> {
        let session = self
            .sessions
            .verify(challenge, signature, now_ms, &self.devices)?;
        self.devices.touch(&session.device, now_ms)?;
        Ok(session)
    }

    pub(crate) fn authenticate(&mut self, bearer: &str, now_ms: u64) -> Option<DeviceId> {
        self.sessions.authenticate(bearer, now_ms)
    }

    /// A token that moves `device`'s access into its Home Screen app.
    pub(crate) fn handoff(&mut self, device: &DeviceId, now_ms: u64) -> Result<Handoff> {
        if self.devices.get(device).is_none() {
            return Err(Error::UnknownDevice);
        }
        self.handoffs.mint(device, now_ms)
    }

    /// The Home Screen app at `client` redeems a handoff with its own key:
    /// the device keeps its ID and name, takes the new key, and the
    /// browser's sessions end.
    pub(crate) fn redeem(
        &mut self,
        client: IpAddr,
        token: &str,
        public_key: &str,
        now_ms: u64,
    ) -> Result<DeviceId> {
        if !self.pair_limit.allow(client, now_ms) {
            return Err(Error::RateLimited);
        }
        let public_key = PublicKey::parse(public_key)?;
        let device = self
            .handoffs
            .redeem(token, now_ms)?
            .ok_or(Error::InvalidToken)?;
        self.devices.rekey(&device, public_key)?;
        self.sessions.end_device(&device);
        self.revoked();
        Ok(device)
    }

    /// Forgets a device and ends its sessions at once.
    pub(crate) fn revoke(&mut self, device: &DeviceId) -> Result<bool> {
        // The device goes either way; a stale digest only names a device
        // that no longer exists.
        let _ = self.handoffs.forget(device);
        self.sessions.end_device(device);
        self.revoked();
        let revoked = self.devices.revoke(device);
        self.sync_apps();
        revoked
    }

    /// Forgets devices unseen for `idle_ms` and ends their sessions.
    pub(crate) fn expire_idle(&mut self, now_ms: u64, idle_ms: u64) -> Result<Vec<DeviceId>> {
        let expired = self.devices.expire_idle(now_ms, idle_ms)?;
        for device in &expired {
            self.sessions.end_device(device);
        }
        if !expired.is_empty() {
            self.revoked();
            self.sync_apps();
        }
        Ok(expired)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

//! Sessions: a paired device signs a fresh challenge with its key and gets
//! a bearer token. Only the token's SHA-256 is kept.

use super::{
    Devices, Error, Result, SECRET_LEN, decode_fixed, encode, random_bytes, random_id, sha256,
};
use crate::control::DeviceId;
use p256::ecdsa::{Signature, signature::Verifier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How long a challenge can be answered.
pub(crate) const CHALLENGE_TTL_MS: u64 = 60_000;
/// How long a session's bearer token is accepted.
pub(crate) const SESSION_TTL_MS: u64 = 12 * 60 * 60 * 1000;
/// Unanswered challenges kept per device; a new one evicts the oldest.
pub(crate) const MAX_CHALLENGES_PER_DEVICE: usize = 8;
/// Sessions kept per device; a new one ends the oldest.
const MAX_SESSIONS_PER_DEVICE: usize = 16;
/// What a device signs before the challenge nonce, so its key's signature
/// over a nonce means nothing elsewhere.
const SIGNED_PREFIX: &[u8] = b"herdr-remote-session\0";
/// Length of an IEEE P1363 P-256 signature: r || s.
const SIGNATURE_LEN: usize = 64;

/// A challenge, named by the daemon.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ChallengeId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Challenge {
    pub(crate) id: ChallengeId,
    /// 32 random bytes, base64url.
    pub(crate) nonce: String,
    pub(crate) expires_ms: u64,
}

/// A started session. The token is shown once, to the device.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Session {
    /// 32 random bytes, base64url: the bearer token.
    pub(crate) token: String,
    pub(crate) device: DeviceId,
    pub(crate) expires_ms: u64,
}

#[derive(Debug)]
struct Pending {
    device: DeviceId,
    nonce: [u8; SECRET_LEN],
    issued_ms: u64,
    expires_ms: u64,
}

#[derive(Debug)]
struct Live {
    device: DeviceId,
    issued_ms: u64,
    expires_ms: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Sessions {
    challenges: HashMap<ChallengeId, Pending>,
    /// Keyed by SHA-256 of the bearer token.
    sessions: HashMap<[u8; 32], Live>,
}

impl Sessions {
    /// Issues a single-use challenge to a paired device.
    pub(crate) fn challenge(
        &mut self,
        device: &DeviceId,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<Challenge> {
        if devices.get(device).is_none() {
            return Err(Error::UnknownDevice);
        }
        self.challenges
            .retain(|_, pending| pending.expires_ms > now_ms);
        let outstanding = self
            .challenges
            .iter()
            .filter(|(_, pending)| &pending.device == device);
        if outstanding.clone().count() >= MAX_CHALLENGES_PER_DEVICE
            && let Some(oldest) = outstanding
                .min_by_key(|(_, pending)| pending.issued_ms)
                .map(|(id, _)| id.clone())
        {
            self.challenges.remove(&oldest);
        }
        let nonce = random_bytes::<SECRET_LEN>()?;
        let id = ChallengeId(random_id()?);
        let expires_ms = now_ms.saturating_add(CHALLENGE_TTL_MS);
        self.challenges.insert(
            id.clone(),
            Pending {
                device: device.clone(),
                nonce,
                issued_ms: now_ms,
                expires_ms,
            },
        );
        Ok(Challenge {
            id,
            nonce: encode(&nonce),
            expires_ms,
        })
    }

    /// Checks the device's signature over the challenge and starts a
    /// session. The challenge is used up whatever the outcome.
    pub(crate) fn verify(
        &mut self,
        challenge: &ChallengeId,
        signature: &str,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<Session> {
        let pending = self
            .challenges
            .remove(challenge)
            .filter(|pending| pending.expires_ms > now_ms)
            .ok_or(Error::UnknownChallenge)?;
        let signature: [u8; SIGNATURE_LEN] =
            decode_fixed(signature).map_err(|_| Error::InvalidSignature)?;
        let signature = Signature::from_slice(&signature).map_err(|_| Error::InvalidSignature)?;
        let key = devices
            .get(&pending.device)
            .ok_or(Error::UnknownDevice)?
            .public_key
            .verifying_key()?;
        key.verify(&signed_message(&pending.nonce), &signature)
            .map_err(|_| Error::InvalidSignature)?;
        self.start(pending.device, now_ms)
    }

    /// The device a bearer token belongs to, while its session lasts.
    pub(crate) fn authenticate(&mut self, bearer: &str, now_ms: u64) -> Option<DeviceId> {
        let token: [u8; SECRET_LEN] = decode_fixed(bearer).ok()?;
        let digest = sha256(&[&token]);
        let live = self.sessions.get(&digest)?;
        if live.expires_ms <= now_ms {
            self.sessions.remove(&digest);
            return None;
        }
        Some(live.device.clone())
    }

    /// Ends every session and drops every challenge of a device.
    pub(crate) fn end_device(&mut self, device: &DeviceId) {
        self.sessions.retain(|_, live| &live.device != device);
        self.challenges
            .retain(|_, pending| &pending.device != device);
    }

    fn start(&mut self, device: DeviceId, now_ms: u64) -> Result<Session> {
        self.sessions.retain(|_, live| live.expires_ms > now_ms);
        let owned = self
            .sessions
            .iter()
            .filter(|(_, live)| live.device == device);
        if owned.clone().count() >= MAX_SESSIONS_PER_DEVICE
            && let Some(oldest) = owned
                .min_by_key(|(_, live)| live.issued_ms)
                .map(|(digest, _)| *digest)
        {
            self.sessions.remove(&oldest);
        }
        let token = random_bytes::<SECRET_LEN>()?;
        let expires_ms = now_ms.saturating_add(SESSION_TTL_MS);
        self.sessions.insert(
            sha256(&[&token]),
            Live {
                device: device.clone(),
                issued_ms: now_ms,
                expires_ms,
            },
        );
        Ok(Session {
            token: encode(&token),
            device,
            expires_ms,
        })
    }
}

/// The bytes a device signs for a challenge's nonce.
pub(crate) fn signed_message(nonce: &[u8]) -> Vec<u8> {
    [SIGNED_PREFIX, nonce].concat()
}

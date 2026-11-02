//! Presence: a paired phone near the computer over Bluetooth proves it is
//! itself by signing a fresh nonce with its device key, under its own
//! prefix so no other signature serves (`docs/ios-app-plan.md`, "Presence,
//! exactly"). Herdr UI relays the steps from the `herdr-nearby` helper.
//!
//! - The phone's `hello` names no device: it carries a hint that changes
//!   daily, so a listener cannot follow a phone by its id.
//! - The computer signs its challenge with its CA key before the phone
//!   signs anything (the server side does that, [`challenge_message`]), so
//!   a peripheral that is not the paired computer gets no signature.
//! - The phone's signature covers the CA fingerprint, so a proof made for
//!   one computer is worth nothing to another.
//! - Every failure reads the same ([`Error::UnknownPresence`]).

use super::{
    Devices, Error, Result, SECRET_LEN, constant_time_eq, decode_fixed, random_bytes, sha256,
};
use crate::control::DeviceId;
use p256::ecdsa::{Signature, signature::Verifier};

/// How long a presence nonce can be answered.
pub(crate) const NONCE_TTL_MS: u64 = 30_000;
/// Nonces waiting per device, so a forged hello cannot cancel the real
/// phone's; the oldest goes beyond this.
pub(crate) const MAX_PENDING_PER_DEVICE: usize = 4;
/// Nonces waiting in all.
const MAX_PENDING: usize = 64;
/// What a phone signs before the CA fingerprint and the nonce.
pub(crate) const PREFIX: &[u8] = b"herdr-presence-v1";
/// What the computer signs before the nonce and the device id.
pub(crate) const CHALLENGE_PREFIX: &[u8] = b"herdr-presence-challenge-v1";
/// What a hint hashes before the device id and the day.
pub(crate) const HINT_PREFIX: &[u8] = b"herdr-presence-hint-v1";
/// Bytes of a hint.
pub(crate) const HINT_LEN: usize = 16;
const DAY_MS: u64 = 86_400_000;
/// Length of an IEEE P1363 P-256 signature: r || s.
const SIGNATURE_LEN: usize = 64;

#[derive(Debug)]
struct Pending {
    device: DeviceId,
    nonce: [u8; SECRET_LEN],
    expires_ms: u64,
}

/// Nonces given out and not yet answered, oldest first.
#[derive(Debug, Default)]
pub(crate) struct Presences {
    pending: Vec<Pending>,
}

/// A challenge to send: the device the hint named and the nonce for it.
#[derive(Debug)]
pub(crate) struct Challenge {
    pub(crate) device: DeviceId,
    pub(crate) nonce: [u8; SECRET_LEN],
}

impl Presences {
    /// A fresh nonce for the device whose hint is `hint` (base64url), for
    /// today, yesterday, or tomorrow, so clocks a little apart agree.
    pub(crate) fn challenge(
        &mut self,
        hint: &str,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<Challenge> {
        let hint: [u8; HINT_LEN] = decode_fixed(hint).map_err(|_| Error::UnknownPresence)?;
        let today = now_ms / DAY_MS;
        let device = devices
            .ids()
            .find(|device| {
                // Every candidate is compared, so the time taken does not
                // tell which day matched.
                [today.saturating_sub(1), today, today.saturating_add(1)]
                    .into_iter()
                    .fold(false, |found, day| {
                        found | constant_time_eq(&hint, &self::hint(device, day))
                    })
            })
            .cloned()
            .ok_or(Error::UnknownPresence)?;
        self.pending.retain(|pending| pending.expires_ms > now_ms);
        let waiting = self
            .pending
            .iter()
            .filter(|pending| pending.device == device)
            .count();
        if waiting >= MAX_PENDING_PER_DEVICE
            && let Some(oldest) = self
                .pending
                .iter()
                .position(|pending| pending.device == device)
        {
            self.pending.remove(oldest);
        }
        if self.pending.len() >= MAX_PENDING {
            self.pending.remove(0);
        }
        let nonce = random_bytes::<SECRET_LEN>()?;
        self.pending.push(Pending {
            device: device.clone(),
            nonce,
            expires_ms: now_ms.saturating_add(NONCE_TTL_MS),
        });
        Ok(Challenge { device, nonce })
    }

    /// Checks a signature over a nonce this computer gave out, for the CA
    /// with `fingerprint`; answers the device it proves. The nonce is spent
    /// either way.
    pub(crate) fn prove(
        &mut self,
        nonce: &str,
        signature: &str,
        fingerprint: &[u8; 32],
        now_ms: u64,
        devices: &Devices,
    ) -> Result<DeviceId> {
        let given: [u8; SECRET_LEN] = decode_fixed(nonce).map_err(|_| Error::UnknownPresence)?;
        let index = self
            .pending
            .iter()
            .position(|pending| constant_time_eq(&given, &pending.nonce))
            .ok_or(Error::UnknownPresence)?;
        let pending = self.pending.remove(index);
        if pending.expires_ms <= now_ms {
            return Err(Error::UnknownPresence);
        }
        let signature: [u8; SIGNATURE_LEN] =
            decode_fixed(signature).map_err(|_| Error::UnknownPresence)?;
        let signature = Signature::from_slice(&signature).map_err(|_| Error::UnknownPresence)?;
        let key = devices
            .get(&pending.device)
            .ok_or(Error::UnknownPresence)?
            .public_key
            .verifying_key()
            .map_err(|_| Error::UnknownPresence)?;
        key.verify(&message(fingerprint, &pending.nonce), &signature)
            .map_err(|_| Error::UnknownPresence)?;
        Ok(pending.device)
    }
}

/// `device`'s hint on `day` (days since 1970, UTC): the first
/// [`HINT_LEN`] bytes of `SHA-256(HINT_PREFIX || device || day as u32 BE)`.
pub(crate) fn hint(device: &DeviceId, day: u64) -> [u8; HINT_LEN] {
    let day = u32::try_from(day).unwrap_or(u32::MAX).to_be_bytes();
    let digest = sha256(&[HINT_PREFIX, device.0.as_bytes(), &day]);
    let mut hint = [0; HINT_LEN];
    hint.copy_from_slice(&digest[..HINT_LEN]);
    hint
}

/// What the computer signs with its CA key: [`CHALLENGE_PREFIX`], the
/// nonce, then the device id.
pub(crate) fn challenge_message(nonce: &[u8], device: &DeviceId) -> Vec<u8> {
    [CHALLENGE_PREFIX, nonce, device.0.as_bytes()].concat()
}

/// What a phone signs: [`PREFIX`], the CA fingerprint's 32 bytes, then the
/// nonce's 32 bytes.
pub(crate) fn message(fingerprint: &[u8; 32], nonce: &[u8]) -> Vec<u8> {
    [PREFIX, fingerprint, nonce].concat()
}

//! A device credential: the relay's word, for thirty days, that a device
//! passed App Attest. It travels inside handles, so the relay stores none.
//!
//! `credential = base64url(device (16) || environment (1) || expires_s (8, big
//! endian) || HMAC-SHA256(secret, the rest))`

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;
use sheprd_apns::Host;

/// How long a credential lasts, matching the daemon's `device_idle_days`.
pub const LIFETIME_S: u64 = 30 * 24 * 60 * 60;
const DEVICE_LEN: usize = 16;
const FACTS_LEN: usize = DEVICE_LEN + 1 + 8;
const MAC_LEN: usize = 32;

/// A device, by a random id that says nothing of the phone.
pub type DeviceId = [u8; DEVICE_LEN];

/// What a credential vouches for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Credential {
    pub device: DeviceId,
    /// Which APNs environment the app's tokens belong to.
    pub environment: Host,
    /// When it stops working, in seconds since the Unix epoch.
    pub expires_s: u64,
}

/// Issues and checks credentials with the relay's secret.
pub struct Issuer {
    keyed: Hmac<Sha256>,
}

impl Issuer {
    /// The issuer for `secret`, or `None` if it cannot key an HMAC.
    #[must_use]
    pub fn new(secret: &[u8]) -> Option<Self> {
        Some(Self {
            keyed: <Hmac<Sha256>>::new_from_slice(secret).ok()?,
        })
    }

    /// The credential's text for `device` from `now_s`.
    #[must_use]
    pub fn issue(&self, device: DeviceId, environment: Host, now_s: u64) -> String {
        let mut bytes = Vec::with_capacity(FACTS_LEN + MAC_LEN);
        bytes.extend_from_slice(&device);
        bytes.push(match environment {
            Host::Production => 0,
            Host::Sandbox => 1,
        });
        bytes.extend_from_slice(&(now_s + LIFETIME_S).to_be_bytes());
        let mac = self.mac(&bytes).finalize().into_bytes();
        bytes.extend_from_slice(&mac);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// What `text` vouches for, when this relay issued it. Expiry is the
    /// caller's check, since a revocation still wants the date.
    #[must_use]
    pub fn check(&self, text: &str) -> Option<Credential> {
        let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
        if bytes.len() != FACTS_LEN + MAC_LEN {
            return None;
        }
        let (facts, mac) = bytes.split_at(FACTS_LEN);
        self.mac(facts).verify_slice(mac).ok()?;
        let (device, rest) = facts.split_at(DEVICE_LEN);
        let (environment, expires) = rest.split_at(1);
        Some(Credential {
            device: device.try_into().ok()?,
            environment: match environment {
                [0] => Host::Production,
                [1] => Host::Sandbox,
                _ => return None,
            },
            expires_s: u64::from_be_bytes(expires.try_into().ok()?),
        })
    }

    fn mac(&self, facts: &[u8]) -> Hmac<Sha256> {
        let mut mac = self.keyed.clone();
        mac.update(facts);
        mac
    }
}

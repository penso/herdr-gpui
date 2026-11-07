//! A handle: a push token and its credential, sealed by the phone to the
//! relay's public key. Daemons hold handles and never see a token, and the
//! relay stores nothing, so a token exists at rest only inside a handle that
//! only a daemon holds.
//!
//! `handle = KEY_ID "." base64url(seal(json, HANDLE_INFO))`
//!
//! ```json
//! { "credential": "…", "id": "5f1c…", "kind": "activity", "token": "80a1…" }
//! ```
//!
//! The key id says which relay key sealed it, so a handle outlives a change of
//! key for as long as the old key is kept.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use secrecy::SecretString;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use sheprd_apns::{DeviceToken, PushType, seal};

/// The longest handle taken: a 256-byte token as hex and a credential, sealed
/// and in base64, are well within it.
pub const MAX_HANDLE: usize = 2048;
const ID_LEN: usize = 32;
const KEY_ID_BYTES: usize = 4;

/// Which token a handle holds, which decides what it can send.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Activity,
    Alert,
    PushToStart,
}

impl Kind {
    /// Whether this kind of token may carry `push_type`.
    #[must_use]
    pub fn allows(self, push_type: PushType) -> bool {
        match self {
            Self::Activity | Self::PushToStart => push_type == PushType::LiveActivity,
            Self::Alert => matches!(push_type, PushType::Alert | PushType::Background),
        }
    }
}

/// An opened handle. It holds a token, so it has no `Debug`: nothing should
/// be able to print one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub credential: SecretString,
    /// Random, chosen by the phone: what revokes this handle alone.
    pub id: String,
    pub kind: Kind,
    pub token: DeviceToken,
}

/// The relay's private keys, newest first.
pub struct Keyring {
    keys: Vec<(String, SecretKey)>,
}

impl Keyring {
    /// `current` seals new handles; `previous` keeps opening older ones.
    #[must_use]
    pub fn new(current: SecretKey, previous: Option<SecretKey>) -> Self {
        Self {
            keys: std::iter::once(current)
                .chain(previous)
                .map(|key| (key_id(&key), key))
                .collect(),
        }
    }

    /// The handle in `text`, or `None` for anything but a well-formed handle
    /// sealed to one of these keys. Callers learn no more than that.
    #[must_use]
    pub fn open(&self, text: &str) -> Option<Handle> {
        if text.len() > MAX_HANDLE {
            return None;
        }
        let (key_id, sealed) = text.split_once('.')?;
        let (_, key) = self.keys.iter().find(|(id, _)| id == key_id)?;
        let sealed = URL_SAFE_NO_PAD.decode(sealed).ok()?;
        let opened = seal::open(key, &sealed, seal::HANDLE_INFO).ok()?;
        let handle: Handle = serde_json::from_slice(&opened).ok()?;
        valid_id(&handle.id).then_some(handle)
    }
}

/// The id a key's handles name it by: the start of its public key's hash.
#[must_use]
pub fn key_id(key: &SecretKey) -> String {
    Sha256::digest(key.public_key().to_sec1_bytes())
        .iter()
        .take(KEY_ID_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A handle id: 128 bits as lowercase hex.
fn valid_id(id: &str) -> bool {
    id.len() == ID_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

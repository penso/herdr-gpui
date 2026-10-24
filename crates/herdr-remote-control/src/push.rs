//! Web Push to paired phones: VAPID (RFC 8292) sender keys, RFC 8291
//! payload encryption, the HTTPS request to the browser's push service, and
//! the rules for when a chat change is worth a notification.
//!
//! Everything here is synchronous. The server calls [`Sender::send`] inside
//! `spawn_blocking`, and keeps one [`Throttle`] per device.
//!
//! A device only reaches this module through a [`Subscription`], which
//! refuses endpoints outside the known push services, so a paired device
//! cannot make the daemon post to an arbitrary host.

mod encrypt;
mod send;
mod subscription;
mod throttle;
mod triggers;
mod vapid;

use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

pub(crate) use encrypt::{MAX_PAYLOAD, encrypt};
pub(crate) use send::{Delivery, Outcome, Sender, Urgency};
#[cfg(test)]
pub(crate) use send::{Topic, outcome};
pub(crate) use subscription::Subscription;
pub(crate) use throttle::Throttle;
pub use triggers::Privacy;
pub(crate) use triggers::{Chat, Kind, Notification, decide, excerpt};
pub(crate) use vapid::{Vapid, is_private};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the push subscription's auth secret is not 16 bytes of base64url")]
    AuthSecret,
    #[error("could not encrypt the push payload")]
    Encrypt,
    #[error("the push subscription's endpoint is not a URL")]
    Endpoint(#[source] url::ParseError),
    #[error("push endpoint host {host} is not a known push service")]
    EndpointHost { host: String },
    #[error("push endpoints must be https URLs on the default port, without credentials")]
    EndpointScheme,
    #[error("the push endpoint is {len} bytes, more than {max}", max = subscription::MAX_ENDPOINT)]
    EndpointTooLong { len: usize },
    #[error("could not send the push")]
    Http(#[from] ureq::Error),
    #[error("could not encode the push payload")]
    Json(#[from] serde_json::Error),
    #[error("could not read the VAPID key {}", path.display())]
    KeyDecode {
        path: PathBuf,
        #[source]
        source: p256::elliptic_curve::DecodeError,
    },
    #[error("could not encode the VAPID key")]
    KeyEncode(#[source] p256::elliptic_curve::Error),
    #[error("the VAPID key {} is open to other users", path.display())]
    KeyNotPrivate { path: PathBuf },
    #[error("could not store the VAPID key at {}", path.display())]
    KeyStore {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the push payload is {len} bytes, more than {max}", max = MAX_PAYLOAD)]
    PayloadTooLarge { len: usize },
    #[error("the push subscription's p256dh key is not a P-256 point in base64url")]
    PublicKey,
    #[error("could not get random bytes for push encryption")]
    Random(#[source] getrandom::Error),
    #[error("could not sign the VAPID token")]
    Sign(#[source] p256::ecdsa::Error),
    #[error("the VAPID subject must be a mailto: or https: URL")]
    Subject,
    // keep-sorted end
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// What the service worker receives, as JSON, once the browser decrypts a
/// push. It shows `title` and `body` under `tag`, replacing an older
/// notification with the same tag, opens `url` when tapped, and sets the
/// app icon's badge to `badge`. With `clear` set it closes the notification
/// tagged `tag` instead, and `title` and `body` are empty.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Message {
    /// Questions waiting across every chat, for the Badging API.
    pub badge: u32,
    pub body: String,
    pub clear: bool,
    pub kind: Kind,
    /// One per chat, such as `chat-PANE`.
    pub tag: String,
    pub title: String,
    /// The page to open: `/chat/PANE`, relative to the app's origin.
    pub url: String,
}

#[cfg(test)]
mod tests;

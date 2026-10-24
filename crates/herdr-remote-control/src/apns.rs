//! Live Activities on paired iPhones through the Apple Push Notification
//! service: the companion app in `ios/` shows one activity per running chat
//! on the lock screen and in the Dynamic Island, and the daemon starts,
//! updates, and ends it with `liveactivity` pushes.
//!
//! - [`Config`]: the user's own APNs key (`STATE_DIR/apns/AuthKey.p8`) and
//!   `config.json`. Without them the feature is off.
//! - [`ProviderToken`]: the ES256 JWT APNs authenticates the daemon by,
//!   refreshed every fifty minutes.
//! - [`decide`]: pure, from the same inputs as [`crate::push::decide`],
//!   whether a chat change starts, updates, or ends the chat's activity.
//! - [`Pacer`]: holds back step updates so a busy turn does not spend the
//!   phone's Live Activity budget.
//! - [`Registration`]: the push tokens one device sent, kept in the device
//!   store's opaque slot as JSON.
//! - [`Sender`]: the HTTP/2 request to `api.push.apple.com` (or the
//!   sandbox), on the daemon's Tokio runtime.
//!
//! A device only ever supplies hex tokens: the host is one of Apple's two,
//! chosen by the user's config, so a paired device cannot make the daemon
//! post anywhere else.

mod config;
mod decide;
mod jwt;
mod pacer;
mod payload;
mod send;
mod tokens;

use std::{io, path::PathBuf};

pub(crate) use config::Config;
pub(crate) use decide::decide;
pub(crate) use jwt::ProviderToken;
pub(crate) use pacer::Pacer;
#[cfg(test)]
pub(crate) use payload::ATTRIBUTES_TYPE;
pub(crate) use payload::{Alert, Attributes, ContentState, Event, Live, Phase};
pub(crate) use send::{Host, Outcome, Sender};
#[cfg(test)]
pub(crate) use send::{Priority, outcome};
#[cfg(test)]
pub(crate) use tokens::ActivityToken;
pub(crate) use tokens::{DeviceToken, Registration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("could not read the APNs response")]
    Body(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("{field} in the APNs config is not valid")]
    ConfigField { field: &'static str },
    #[error("{} is not a valid APNs config", path.display())]
    ConfigFormat {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not read the APNs config {}", path.display())]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not connect to APNs")]
    Connect(#[source] io::Error),
    #[error("an APNs device token must be 16 to 256 bytes of hex")]
    DeviceToken,
    #[error("APNs host {host} is not one of Apple's push hosts")]
    Host { host: String },
    #[error("the APNs request failed")]
    Http(#[from] hyper::Error),
    #[error("could not encode the APNs payload")]
    Json(#[from] serde_json::Error),
    #[error("could not read the APNs key {}", path.display())]
    KeyDecode {
        path: PathBuf,
        #[source]
        source: p256::pkcs8::Error,
    },
    #[error("the APNs key {} is open to other users", path.display())]
    KeyNotPrivate { path: PathBuf },
    #[error("could not read the APNs key {}", path.display())]
    KeyRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("a Live Activity's pane must be 1 to 128 characters with no control characters")]
    Pane,
    #[error("the APNs payload is {len} bytes, more than {max}", max = payload::MAX_PAYLOAD)]
    PayloadTooLarge { len: usize },
    #[error("could not build the APNs request")]
    Request(#[from] hyper::http::Error),
    #[error("could not sign the APNs provider token")]
    Sign(#[source] p256::ecdsa::Error),
    #[error("APNs did not answer in time")]
    Timeout,
    #[error("could not set up TLS for APNs")]
    Tls(#[source] rustls::Error),
    #[error("{len} Live Activity tokens, more than {max}", max = tokens::MAX_ACTIVITIES)]
    TooManyActivities { len: usize },
    // keep-sorted end
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;

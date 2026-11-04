//! A phone's push subscription, as the browser's `PushSubscription.toJSON()`
//! gives it: `{endpoint, keys: {p256dh, auth}}`. Deserializing validates it,
//! so a `Subscription` always names a known push service over https and
//! carries usable keys.

use super::{Error, Result, vapid::B64};
use base64::Engine as _;
use p256::PublicKey;
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;

/// The longest endpoint accepted; real ones are a few hundred bytes.
pub(crate) const MAX_ENDPOINT: usize = 2048;
/// The user agent's authentication secret (RFC 8291 section 3.2).
pub(super) const AUTH_LEN: usize = 16;
/// An uncompressed P-256 point: `0x04 || X || Y`.
pub(super) const POINT_LEN: usize = 65;

/// Push services the daemon posts to, by exact host.
const HOSTS: &[&str] = &[
    // keep-sorted start
    "android.googleapis.com",
    "fcm.googleapis.com",
    "updates.push.services.mozilla.com",
    // keep-sorted end
];

/// Push services the daemon posts to, by domain: any host under them.
const DOMAINS: &[&str] = &[
    // keep-sorted start
    ".notify.windows.com",
    ".push.apple.com",
    ".push.services.mozilla.com",
    // keep-sorted end
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(into = "Json", try_from = "Json")]
pub(crate) struct Subscription {
    pub(super) auth: [u8; AUTH_LEN],
    pub(super) endpoint: Endpoint,
    pub(super) p256dh: PublicKey,
}

/// The endpoint URL. Its path is a capability for pushing to the device, so
/// `Debug` shows only the host.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Endpoint(pub(super) Url);

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Endpoint({})", self.0.host_str().unwrap_or_default())
    }
}

#[derive(Serialize, Deserialize)]
struct Json {
    endpoint: String,
    keys: Keys,
}

#[derive(Serialize, Deserialize)]
struct Keys {
    auth: String,
    p256dh: String,
}

impl Subscription {
    /// The push service's origin, the audience of its VAPID token.
    pub(crate) fn audience(&self) -> String {
        self.endpoint.0.origin().ascii_serialization()
    }

    pub(crate) fn endpoint(&self) -> &str {
        self.endpoint.0.as_str()
    }
}

impl TryFrom<Json> for Subscription {
    type Error = Error;

    fn try_from(json: Json) -> Result<Self> {
        Self::parse(&json.endpoint, &json.keys.p256dh, &json.keys.auth)
    }
}

impl Subscription {
    /// Checks a subscription's parts, as `toJSON()` spells them.
    pub(super) fn parse(endpoint: &str, p256dh: &str, auth: &str) -> Result<Self> {
        let endpoint = self::endpoint(endpoint)?;
        let auth = B64
            .decode(auth.as_bytes())
            .ok()
            .and_then(|bytes| <[u8; AUTH_LEN]>::try_from(bytes).ok())
            .ok_or(Error::AuthSecret)?;
        let p256dh = B64
            .decode(p256dh.as_bytes())
            .ok()
            .filter(|bytes| bytes.len() == POINT_LEN)
            .and_then(|bytes| PublicKey::from_sec1_bytes(&bytes).ok())
            .ok_or(Error::PublicKey)?;
        Ok(Self {
            auth,
            endpoint,
            p256dh,
        })
    }
}

impl From<Subscription> for Json {
    fn from(subscription: Subscription) -> Self {
        Self {
            endpoint: subscription.endpoint.0.into(),
            keys: Keys {
                auth: B64.encode(subscription.auth),
                p256dh: B64.encode(subscription.p256dh.to_sec1_bytes()),
            },
        }
    }
}

fn endpoint(text: &str) -> Result<Endpoint> {
    if text.len() > MAX_ENDPOINT {
        return Err(Error::EndpointTooLong { len: text.len() });
    }
    let url = Url::parse(text).map_err(Error::Endpoint)?;
    let plain = url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    if !plain {
        return Err(Error::EndpointScheme);
    }
    let Some(url::Host::Domain(host)) = url.host() else {
        return Err(Error::EndpointHost {
            host: url.host_str().unwrap_or_default().to_owned(),
        });
    };
    if !is_push_service(host) {
        return Err(Error::EndpointHost {
            host: host.to_owned(),
        });
    }
    Ok(Endpoint(url))
}

/// Whether `host` (lowercase, as `Url` gives it) is a known push service.
pub(super) fn is_push_service(host: &str) -> bool {
    HOSTS.contains(&host)
        || DOMAINS.iter().any(|domain| {
            host.strip_suffix(domain)
                .is_some_and(|label| !label.is_empty() && !label.ends_with('.'))
        })
}

//! The HTTPS request that hands an encrypted push to the browser's push
//! service (RFC 8030). Blocking: the server calls it from `spawn_blocking`.

use super::{Error, Kind, Message, Notification, Result, Subscription, Vapid, encrypt, vapid};
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long one push may take, connecting included.
const TIMEOUT: Duration = Duration::from_secs(10);
/// The longest `Topic` header RFC 8030 allows.
const MAX_TOPIC: usize = 32;

/// How soon the push service should wake the device (RFC 8030 section 5.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Urgency {
    Normal,
    High,
}

impl Urgency {
    pub(crate) fn header(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

/// A `Topic` header: at most 32 base64url characters. A push service keeps
/// only the newest undelivered push of a topic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Topic(String);

impl Topic {
    /// A topic derived from a notification tag, so one chat's pushes replace
    /// each other while the phone is offline, without the tag's text.
    pub(crate) fn from_tag(tag: &str) -> Self {
        let digest = Sha256::digest(tag.as_bytes());
        let mut text = vapid::B64.encode(digest);
        text.truncate(MAX_TOPIC);
        Self(text)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a push is delivered, apart from its content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Delivery {
    pub topic: Option<Topic>,
    /// How long the push service keeps it for an offline device.
    pub ttl_s: u32,
    pub urgency: Urgency,
}

impl Delivery {
    /// A question waits a day for the phone; a finished turn or an error is
    /// stale after an hour. A clear is worth delivering only soon.
    pub(crate) fn for_notification(notification: &Notification) -> Self {
        let ttl_s = match (notification.clear, notification.kind) {
            (true, _) => 60 * 60,
            (false, Kind::Question) => 24 * 60 * 60,
            (false, Kind::Error | Kind::TurnEnded) => 60 * 60,
        };
        Self {
            topic: Some(Topic::from_tag(&notification.tag)),
            ttl_s,
            urgency: notification.urgency,
        }
    }
}

/// What the push service did with a push.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Delivered,
    /// The subscription expired or was removed: drop it.
    Gone,
    /// Too many pushes; wait `retry_after` when the service says how long.
    RateLimited {
        retry_after: Option<Duration>,
    },
    /// Any other refusal, by HTTP status.
    Rejected(u16),
}

/// The push service's answer, from its status and `Retry-After` header.
pub(crate) fn outcome(status: u16, retry_after: Option<&str>) -> Outcome {
    match status {
        200..=299 => Outcome::Delivered,
        404 | 410 => Outcome::Gone,
        429 => Outcome::RateLimited {
            retry_after: retry_after
                .and_then(|value| value.trim().parse().ok())
                .map(Duration::from_secs),
        },
        status => Outcome::Rejected(status),
    }
}

/// Sends pushes signed with one VAPID key, reusing connections.
#[derive(Debug)]
pub(crate) struct Sender {
    agent: ureq::Agent,
    subject: String,
    vapid: Vapid,
}

impl Sender {
    /// `subject` is the `mailto:` or `https:` contact push services see.
    pub(crate) fn new(vapid: Vapid, subject: impl Into<String>) -> Result<Self> {
        let subject = subject.into();
        vapid::check_subject(&subject)?;
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            // A push service answers; it never sends the daemon elsewhere.
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            agent,
            subject,
            vapid,
        })
    }

    pub(crate) fn vapid(&self) -> &Vapid {
        &self.vapid
    }

    /// Encrypts `message` for `subscription` and posts it. Blocking, up to
    /// ten seconds.
    pub(crate) fn send(
        &self,
        subscription: &Subscription,
        message: &Message,
        delivery: &Delivery,
    ) -> Result<Outcome> {
        let now_s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let push = self.prepare(subscription, message, delivery, now_s)?;
        let request = push.headers.into_iter().fold(
            self.agent.post(subscription.endpoint()),
            |request, (name, value)| request.header(name, value),
        );
        let response = request.send(&push.body[..]).map_err(Error::Http)?;
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok());
        Ok(outcome(response.status().as_u16(), retry_after))
    }

    /// The headers and encrypted body of a push, signed at `now_s`.
    pub(super) fn prepare(
        &self,
        subscription: &Subscription,
        message: &Message,
        delivery: &Delivery,
        now_s: u64,
    ) -> Result<Prepared> {
        let body = encrypt(subscription, &serde_json::to_vec(message)?)?;
        let authorization =
            self.vapid
                .authorization(&subscription.audience(), &self.subject, now_s)?;
        let mut headers = vec![
            ("Authorization", authorization),
            ("Content-Encoding", "aes128gcm".to_owned()),
            ("Content-Type", "application/octet-stream".to_owned()),
            ("TTL", delivery.ttl_s.to_string()),
            ("Urgency", delivery.urgency.header().to_owned()),
        ];
        if let Some(topic) = &delivery.topic {
            headers.push(("Topic", topic.as_str().to_owned()));
        }
        Ok(Prepared { body, headers })
    }
}

/// A push ready to post.
#[derive(Debug)]
pub(super) struct Prepared {
    pub(super) body: Vec<u8>,
    pub(super) headers: Vec<(&'static str, String)>,
}

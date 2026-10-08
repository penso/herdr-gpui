//! Forwards agent attention notices to the user's phone through ntfy (hosted
//! or self-hosted) or Pushover.
//!
//! Only notices that pass the shared notification policy in [`super::tick`]
//! are forwarded, and only while the window is not focused. The payload is
//! the notice's sanitized title and a short bounded summary, never terminal
//! output. Requests run on one worker thread fed through a bounded channel;
//! the UI thread only rate-limits and enqueues, and a full queue drops.

use crate::{Error, Result};
use herdr_client::protocol::SemanticNotificationKind as Kind;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    sync::mpsc::{SyncSender, TrySendError},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const DEFAULT_NTFY: &str = "https://ntfy.sh/";
const PUSHOVER_URL: &str = "https://api.pushover.net/1/messages.json";
const TIMEOUT: Duration = Duration::from_secs(10);
/// Kept well under both services' limits so a phone shows it whole.
const TITLE_LIMIT: usize = 80;
const BODY_LIMIT: usize = 160;
/// At most this many pushes in [`RATE_WINDOW`]; the rest are dropped.
const RATE_LIMIT: usize = 6;
const RATE_WINDOW: Duration = Duration::from_secs(60);
/// How long the worker stops sending after the service answers 429.
const BACKOFF: Duration = Duration::from_secs(60);
const QUEUE: usize = 8;
/// Two windows attached to one host see the same event. The second copy
/// within this interval is a duplicate, as for OS notifications.
const DUPLICATE_WINDOW: Duration = Duration::from_secs(2);
const RECENT_LIMIT: usize = 64;

/// `[phone]` in the GUI config. Secrets stay in [`SecretString`]s and are
/// exposed only while a request body or header is built.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhoneConfig {
    pub enabled: bool,
    /// Which service to use when both tables are filled in.
    pub service: Option<ServiceKind>,
    /// Forward "needs attention" notices: an agent is blocked on input.
    pub blocked: bool,
    /// Forward "finished" notices: an agent is done.
    pub done: bool,
    pub ntfy: NtfySettings,
    pub pushover: PushoverSettings,
}

impl Default for PhoneConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service: None,
            blocked: true,
            done: true,
            ntfy: NtfySettings::default(),
            pushover: PushoverSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Ntfy,
    Pushover,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NtfySettings {
    /// Defaults to the public `https://ntfy.sh`.
    pub server: Option<String>,
    /// Anyone who knows a public topic can read it, so it is a secret.
    pub topic: Option<SecretString>,
    /// An access token for a protected topic or a self-hosted server.
    pub token: Option<SecretString>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PushoverSettings {
    /// The application's API token.
    pub token: Option<SecretString>,
    /// The user or group key that receives the message.
    pub user: Option<SecretString>,
}

/// Which notice kinds go to the phone, already folded with `enabled` and a
/// usable service so [`super::tick`] needs no secrets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PhoneEvents {
    pub blocked: bool,
    pub done: bool,
}

impl PhoneEvents {
    pub(crate) fn wants(self, kind: Kind) -> bool {
        match kind {
            Kind::NeedsAttention => self.blocked,
            Kind::Finished => self.done,
            Kind::Custom | Kind::UpdateInstalled => false,
        }
    }
}

/// A resolved destination, cheap to clone into the worker.
#[derive(Clone, Debug)]
pub(crate) enum Service {
    Ntfy {
        /// The server root, ending in `/`: JSON publishes go there, so the
        /// topic travels in the body rather than the URL.
        server: url::Url,
        topic: SecretString,
        token: Option<SecretString>,
    },
    Pushover {
        token: SecretString,
        user: SecretString,
    },
}

impl Service {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Ntfy { .. } => "ntfy",
            Self::Pushover { .. } => "Pushover",
        }
    }

    /// Where messages go, without the topic or any key.
    pub(crate) fn host(&self) -> String {
        match self {
            Self::Ntfy { server, .. } => server.host_str().unwrap_or_default().to_owned(),
            Self::Pushover { .. } => "api.pushover.net".into(),
        }
    }
}

fn present(value: Option<&SecretString>) -> Option<&SecretString> {
    value.filter(|value| !value.expose_secret().trim().is_empty())
}

impl PhoneConfig {
    /// Checks what can be checked without sending, so a typo is reported
    /// when the config loads rather than when an agent first blocks.
    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(server) = &self.ntfy.server {
            server_url(server)?;
        }
        if let Some(topic) = present(self.ntfy.topic.as_ref()) {
            let topic = topic.expose_secret();
            if topic.len() > 64
                || !topic
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(Error::PhoneTopic);
            }
        }
        Ok(())
    }

    /// The configured destination, whether or not forwarding is enabled.
    pub(crate) fn service(&self) -> Result<Service> {
        let ntfy = present(self.ntfy.topic.as_ref()).map(|topic| (topic, &self.ntfy));
        let pushover =
            present(self.pushover.token.as_ref()).zip(present(self.pushover.user.as_ref()));
        let kind = match (self.service, ntfy.is_some(), pushover.is_some()) {
            (Some(kind), _, _) => kind,
            (None, true, false) => ServiceKind::Ntfy,
            (None, false, true) => ServiceKind::Pushover,
            (None, true, true) => return Err(Error::PhoneServiceAmbiguous),
            (None, false, false) => return Err(Error::PhoneNotConfigured),
        };
        match kind {
            ServiceKind::Ntfy => {
                let (topic, settings) = ntfy.ok_or(Error::PhoneNotConfigured)?;
                Ok(Service::Ntfy {
                    server: server_url(settings.server.as_deref().unwrap_or(DEFAULT_NTFY))?,
                    topic: topic.clone(),
                    token: present(settings.token.as_ref()).cloned(),
                })
            }
            ServiceKind::Pushover => {
                let (token, user) = pushover.ok_or(Error::PhoneNotConfigured)?;
                Ok(Service::Pushover {
                    token: token.clone(),
                    user: user.clone(),
                })
            }
        }
    }

    pub(crate) fn events(&self) -> PhoneEvents {
        if !self.enabled || self.service().is_err() {
            return PhoneEvents::default();
        }
        PhoneEvents {
            blocked: self.blocked,
            done: self.done,
        }
    }
}

fn server_url(text: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(text.trim()).map_err(|_| Error::PhoneServer)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::PhoneServer);
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Blocked,
    Done,
    Test,
}

impl Event {
    pub(crate) fn from_kind(kind: Kind) -> Option<Self> {
        match kind {
            Kind::NeedsAttention => Some(Self::Blocked),
            Kind::Finished => Some(Self::Done),
            Kind::Custom | Kind::UpdateInstalled => None,
        }
    }
}

/// What reaches the phone: bounded, control-free text only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Message {
    pub event: Event,
    pub title: String,
    pub body: String,
}

impl Message {
    /// `label` names the host when the window has several.
    pub(crate) fn new(event: Event, title: &str, body: Option<&str>, label: Option<&str>) -> Self {
        let title = super::safe_text(title, TITLE_LIMIT);
        let summary = match (label, body) {
            (Some(label), Some(body)) => format!("{label}: {body}"),
            (Some(text), None) | (None, Some(text)) => text.to_owned(),
            (None, None) => String::new(),
        };
        let mut body = super::safe_text(&summary, BODY_LIMIT);
        if body.trim().is_empty() {
            // Both services refuse an empty message.
            body = match event {
                Event::Blocked => "An agent is waiting for you.",
                Event::Done => "An agent finished.",
                Event::Test => "Test notification.",
            }
            .into();
        }
        Self { event, title, body }
    }

    pub(crate) fn test() -> Self {
        Self::new(
            Event::Test,
            "Herdr test",
            Some("Phone notifications are working."),
            None,
        )
    }
}

/// One HTTP request, built without I/O so the payload is testable. The body
/// carries secrets and is wiped on drop.
pub(crate) struct Request {
    pub url: String,
    pub authorization: Option<Zeroizing<String>>,
    pub body: Zeroizing<Vec<u8>>,
}

pub(crate) fn request(service: &Service, message: &Message) -> Result<Request> {
    match service {
        Service::Ntfy {
            server,
            topic,
            token,
        } => {
            let (priority, tag) = match message.event {
                Event::Blocked => (4, "warning"),
                Event::Done => (3, "white_check_mark"),
                Event::Test => (3, "test_tube"),
            };
            let body = serde_json::to_vec(&serde_json::json!({
                "topic": topic.expose_secret(),
                "title": message.title,
                "message": message.body,
                "priority": priority,
                "tags": [tag],
            }))?;
            Ok(Request {
                url: server.as_str().to_owned(),
                authorization: token
                    .as_ref()
                    .map(|token| Zeroizing::new(format!("Bearer {}", token.expose_secret()))),
                body: Zeroizing::new(body),
            })
        }
        Service::Pushover { token, user } => {
            let body = serde_json::to_vec(&serde_json::json!({
                "token": token.expose_secret(),
                "user": user.expose_secret(),
                "title": message.title,
                "message": message.body,
            }))?;
            Ok(Request {
                url: PUSHOVER_URL.into(),
                authorization: None,
                body: Zeroizing::new(body),
            })
        }
    }
}

/// Sends one message, blocking for at most [`TIMEOUT`]. Never call it on the
/// UI thread. The response body is not read: neither service needs it, and
/// Pushover may echo account details in it.
pub(crate) fn send(service: &Service, message: &Message) -> Result<()> {
    let request = request(service, message)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut call = agent
        .post(&request.url)
        .header("Content-Type", "application/json");
    if let Some(authorization) = &request.authorization {
        let mut value =
            ureq::http::HeaderValue::from_str(authorization).map_err(Error::PhoneHeader)?;
        value.set_sensitive(true);
        call = call.header("Authorization", value);
    }
    let response = call
        .send(request.body.as_slice())
        .map_err(Error::PhoneNetwork)?;
    status(response.status().as_u16())
}

fn status(code: u16) -> Result<()> {
    match code {
        200..=299 => Ok(()),
        401 | 403 => Err(Error::PhoneRejected(code)),
        429 => Err(Error::PhoneRateLimited),
        code => Err(Error::PhoneStatus(code)),
    }
}

/// A sliding window over recent pushes, so a flapping agent cannot flood the
/// phone or exhaust a service quota.
#[derive(Debug, Default)]
pub(crate) struct Limiter {
    sent: VecDeque<Instant>,
}

impl Limiter {
    pub(crate) fn allow(&mut self, now: Instant) -> bool {
        while self
            .sent
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= RATE_WINDOW)
        {
            self.sent.pop_front();
        }
        if self.sent.len() >= RATE_LIMIT {
            return false;
        }
        self.sent.push_back(now);
        true
    }
}

/// Worker-side state: after a 429, drop everything until the backoff ends.
#[derive(Debug, Default)]
struct Backoff {
    until: Option<Instant>,
}

impl Backoff {
    fn run(&mut self, now: Instant, send: impl FnOnce() -> Result<()>) -> Option<Result<()>> {
        if self.until.is_some_and(|until| now < until) {
            return None;
        }
        self.until = None;
        let result = send();
        if matches!(result, Err(Error::PhoneRateLimited)) {
            self.until = Some(now + BACKOFF);
        }
        Some(result)
    }
}

/// App-wide owner of the worker, the rate limit, and cross-window dedup.
/// Only normal launches install it, so tests and fixtures never send.
#[derive(Default)]
pub(crate) struct Dispatcher {
    sender: Option<SyncSender<(Service, Message)>>,
    limiter: Limiter,
    recent: VecDeque<(String, Instant)>,
}

impl gpui::Global for Dispatcher {}

impl Dispatcher {
    /// Whether this push should go out: not a duplicate of another window's
    /// copy, and within the rate limit. Records it when it should.
    pub(crate) fn admit(&mut self, tag: &str, now: Instant) -> bool {
        self.recent
            .retain(|(_, at)| now.saturating_duration_since(*at) < DUPLICATE_WINDOW);
        if self.recent.iter().any(|(recent, _)| recent == tag) {
            return false;
        }
        if !self.limiter.allow(now) {
            tracing::info!(
                category = "phone",
                "phone notification dropped by rate limit"
            );
            return false;
        }
        if self.recent.len() == RECENT_LIMIT {
            self.recent.pop_front();
        }
        self.recent.push_back((tag.to_owned(), now));
        true
    }

    /// Hands a message to the worker without blocking, starting it on first
    /// use. A full queue drops the message.
    pub(crate) fn enqueue(&mut self, service: Service, message: Message) {
        let mut job = (service, message);
        if let Some(sender) = &self.sender {
            match sender.try_send(job) {
                Ok(()) => return,
                Err(TrySendError::Full(_)) => {
                    tracing::info!(category = "phone", "phone notification queue full; dropped");
                    return;
                }
                // The worker died; start a new one below.
                Err(TrySendError::Disconnected(returned)) => job = returned,
            }
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel::<(Service, Message)>(QUEUE);
        let spawned = std::thread::Builder::new()
            .name("herdr-phone".into())
            .spawn(move || {
                let mut backoff = Backoff::default();
                for (service, message) in receiver {
                    match backoff.run(Instant::now(), || send(&service, &message)) {
                        Some(Err(error)) => tracing::warn!(
                            category = "phone",
                            service = service.name(),
                            %error,
                            "phone notification failed"
                        ),
                        None => tracing::info!(
                            category = "phone",
                            "phone notification skipped during rate-limit backoff"
                        ),
                        Some(Ok(())) => {}
                    }
                }
            });
        match spawned {
            Ok(_) => {
                // The fresh queue is empty, so this cannot be full.
                let _ = sender.try_send(job);
                self.sender = Some(sender);
            }
            Err(error) => {
                tracing::warn!(category = "phone", %error, "could not start phone notifier");
            }
        }
    }
}

pub(crate) fn install(cx: &mut gpui::App) {
    cx.set_global(Dispatcher::default());
}

#[cfg(test)]
mod tests;

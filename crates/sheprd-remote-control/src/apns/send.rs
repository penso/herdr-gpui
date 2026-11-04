//! The HTTP/2 request that hands a push to APNs: a Live Activity update, a
//! sealed alert, or a background clear. Async, on the
//! daemon's Tokio runtime: one TLS connection to Apple, reused for every
//! push (APNs asks providers to keep connections open) and opened again
//! when it drops.

use super::{Config, DeviceToken, Error, Live, ProviderToken, Result};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
    client::conn::http2::{self, SendRequest},
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::{ClientConfig, RootCertStore, crypto::ring, pki_types::ServerName};
use serde::Deserialize;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{net::TcpStream, sync::Mutex};
use tokio_rustls::TlsConnector;

/// How long one push may take, connecting included.
const TIMEOUT: Duration = Duration::from_secs(10);
/// The most of an error response read; APNs sends a short JSON reason.
const MAX_RESPONSE: usize = 4096;

/// Apple's two push hosts, the only ones the daemon posts to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Host {
    Production,
    /// For an app built with a development provisioning profile.
    Sandbox,
}

impl Host {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Production => "api.push.apple.com",
            Self::Sandbox => "api.sandbox.push.apple.com",
        }
    }
}

/// `apns-priority`: 10 delivers at once and counts against the app's
/// budget; 5 lets iOS pick a moment that saves power.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Priority {
    Low,
    High,
}

impl Priority {
    pub(crate) fn header(self) -> &'static str {
        match self {
            Self::Low => "5",
            Self::High => "10",
        }
    }
}

/// `apns-push-type`, which also picks the topic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PushType {
    /// A notification the app shows, after its extension opens it.
    Alert,
    /// Wakes the app without showing anything, such as to clear one.
    Background,
    LiveActivity,
}

impl PushType {
    pub(crate) fn header(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::Background => "background",
            Self::LiveActivity => "liveactivity",
        }
    }
}

/// One push's body and the headers that go with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Push {
    pub(crate) body: Vec<u8>,
    /// `apns-collapse-id`: a newer push with the same id replaces the
    /// older one on the phone.
    pub(crate) collapse_id: Option<String>,
    pub(crate) priority: Priority,
    pub(crate) push_type: PushType,
}

/// What APNs did with a push.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Delivered,
    /// The token is no longer valid (`BadDeviceToken`, `Unregistered`, or a
    /// token for another app): drop it.
    Gone,
    /// Too many pushes for this token; try later.
    RateLimited,
    /// Any other refusal, with APNs's reason when it gave one.
    Rejected {
        reason: Option<String>,
        status: u16,
    },
}

#[derive(Deserialize)]
struct Reason {
    reason: String,
}

/// APNs's answer, from its status and JSON body.
pub(crate) fn outcome(status: u16, body: &[u8]) -> Outcome {
    let reason = serde_json::from_slice::<Reason>(body)
        .ok()
        .map(|reason| reason.reason);
    match (status, reason.as_deref()) {
        (200, _) => Outcome::Delivered,
        (410, _) | (400, Some("BadDeviceToken" | "DeviceTokenNotForTopic")) => Outcome::Gone,
        (429, _) => Outcome::RateLimited,
        (status, _) => Outcome::Rejected { reason, status },
    }
}

/// Whether APNs refused the provider token itself, so a new one is due.
fn provider_token_refused(outcome: &Outcome) -> bool {
    matches!(
        outcome,
        Outcome::Rejected { status: 403, reason: Some(reason) }
            if reason == "ExpiredProviderToken" || reason == "InvalidProviderToken"
    )
}

/// The request for `push` to `token`, authorized by `bearer`.
pub(crate) fn request(
    config: &Config,
    token: &DeviceToken,
    push: &Push,
    bearer: &str,
) -> Result<Request<Full<Bytes>>> {
    let topic = match push.push_type {
        PushType::LiveActivity => config.live_activity_topic(),
        PushType::Alert | PushType::Background => config.topic.clone(),
    };
    let mut request = Request::post(format!(
        "https://{}/3/device/{}",
        config.host.name(),
        token.as_str()
    ))
    .header(AUTHORIZATION, format!("bearer {bearer}"))
    .header(CONTENT_TYPE, "application/json")
    .header("apns-push-type", push.push_type.header())
    .header("apns-topic", topic)
    .header("apns-priority", push.priority.header());
    if let Some(id) = &push.collapse_id {
        request = request.header("apns-collapse-id", id);
    }
    Ok(request.body(Full::new(Bytes::from(push.body.clone())))?)
}

/// Sends pushes with the user's APNs key.
#[derive(Debug)]
pub(crate) struct Sender {
    config: Config,
    connection: Mutex<Option<SendRequest<Full<Bytes>>>>,
    tls: Arc<ClientConfig>,
    token: ProviderToken,
}

impl Sender {
    /// The sender for the credentials in `state_dir`, or `None` when the
    /// user set none up. Blocking file I/O.
    pub(crate) fn load(state_dir: &Path) -> Result<Option<Self>> {
        Config::load(state_dir)?.map(Self::new).transpose()
    }

    pub(crate) fn new(config: Config) -> Result<Self> {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let mut tls = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(Error::Tls)?
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"h2".to_vec()];
        let token = ProviderToken::new(
            config.key.clone(),
            config.team_id.clone(),
            config.key_id.clone(),
        );
        Ok(Self {
            config,
            connection: Mutex::new(None),
            tls: Arc::new(tls),
            token,
        })
    }

    /// Sends `live` to `token` at `now_s` (seconds since the Unix epoch),
    /// within ten seconds.
    pub(crate) async fn send(
        &self,
        token: &DeviceToken,
        live: &Live,
        now_s: u64,
    ) -> Result<Outcome> {
        let push = Push {
            body: live.payload(now_s)?,
            collapse_id: None,
            priority: if live.urgent() {
                Priority::High
            } else {
                Priority::Low
            },
            push_type: PushType::LiveActivity,
        };
        self.post(token, &push, now_s).await
    }

    /// Sends `push` to `token` at `now_s`, within ten seconds.
    pub(crate) async fn post(
        &self,
        token: &DeviceToken,
        push: &Push,
        now_s: u64,
    ) -> Result<Outcome> {
        let bearer = self.token.current(now_s)?;
        let exchange = async {
            // A kept connection may have been closed by Apple meanwhile;
            // one retry on a fresh connection covers that.
            let mut fresh = false;
            loop {
                let mut sender = self.sender(fresh).await?;
                let request = request(&self.config, token, push, &bearer)?;
                match send_on(&mut sender, request).await {
                    Ok(response) => return read(response).await,
                    Err(error) if fresh => return Err(error),
                    Err(_) => fresh = true,
                }
            }
        };
        let outcome = tokio::time::timeout(TIMEOUT, exchange)
            .await
            .map_err(|_| Error::Timeout)??;
        if provider_token_refused(&outcome) {
            self.token.invalidate();
        }
        Ok(outcome)
    }

    /// The kept connection, or a new one when there is none, it closed, or
    /// `fresh` asks for one.
    async fn sender(&self, fresh: bool) -> Result<SendRequest<Full<Bytes>>> {
        let mut kept = self.connection.lock().await;
        if !fresh
            && let Some(sender) = kept.as_ref()
            && !sender.is_closed()
        {
            return Ok(sender.clone());
        }
        let sender = self.connect().await?;
        *kept = Some(sender.clone());
        Ok(sender)
    }

    async fn connect(&self) -> Result<SendRequest<Full<Bytes>>> {
        let host = self.config.host.name();
        let name = ServerName::try_from(host).map_err(|_| Error::Host {
            host: host.to_owned(),
        })?;
        let tcp = TcpStream::connect((host, 443))
            .await
            .map_err(Error::Connect)?;
        let tls = TlsConnector::from(self.tls.clone())
            .connect(name, tcp)
            .await
            .map_err(Error::Connect)?;
        let (sender, connection) =
            http2::handshake(TokioExecutor::new(), TokioIo::new(tls)).await?;
        // The connection task ends when Apple or the daemon closes it.
        tokio::spawn(connection);
        Ok(sender)
    }
}

async fn send_on(
    sender: &mut SendRequest<Full<Bytes>>,
    request: Request<Full<Bytes>>,
) -> Result<Response<Incoming>> {
    sender.ready().await?;
    Ok(sender.send_request(request).await?)
}

async fn read(response: Response<Incoming>) -> Result<Outcome> {
    let status = response.status();
    if status == StatusCode::OK {
        return Ok(Outcome::Delivered);
    }
    let body = Limited::new(response.into_body(), MAX_RESPONSE)
        .collect()
        .await
        .map_err(Error::Body)?
        .to_bytes();
    Ok(outcome(status.as_u16(), &body))
}

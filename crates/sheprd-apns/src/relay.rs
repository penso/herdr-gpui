//! The daemon's side of the push relay: what sends a push when the user has
//! no APNs key of their own. Nothing here needs setting up again after the
//! relay restarts: a push carries its own handle, so the relay needs nothing
//! from an earlier call. A connection that dropped is opened again, and one
//! failure is tried again a moment later.
//!
//! Sending twice is safe. A reply lost on the way back can repeat a push,
//! which shows once, since pushes for a chat share a collapse id.
//!
//! The hosts are compiled in, so nothing a device or a caller supplies can
//! send a push anywhere else.

use crate::{Error, Push, Result};
use http_body_util::Full;
use hyper::{
    Method, Request, StatusCode,
    body::Bytes,
    client::conn::http2::{self, SendRequest},
    header::CONTENT_TYPE,
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::{ClientConfig, pki_types::ServerName};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpStream, sync::Mutex};
use tokio_rustls::TlsConnector;

/// How long one try may take, connecting included.
const TIMEOUT: Duration = Duration::from_secs(5);
/// The waits between tries: long enough for a relay restarting behind its
/// proxy to answer, short enough that the push is still worth having.
const RETRIES: [Duration; 2] = [Duration::from_millis(250), Duration::from_secs(1)];

const PUSH: &str = "/v1/push";
const REVOKE: &str = "/v1/revoke";

/// The relay for each APNs environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayHost {
    Production,
    Sandbox,
}

impl RelayHost {
    pub fn name(self) -> &'static str {
        match self {
            Self::Production => "push.sheprd.org",
            Self::Sandbox => "push-sandbox.sheprd.org",
        }
    }
}

/// What came of a call to the relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayOutcome {
    /// The relay did it.
    Delivered,
    /// The phone's app is gone or revoked it: forget the handle.
    Gone,
    /// The handle's credential ran out, or the relay no longer takes it. The
    /// phone makes a new one when it next registers; only it can.
    Stale,
    /// Too much, too fast. Tried once: more would only make it worse.
    Limited,
    /// The relay refused what was sent, which trying again cannot change.
    Invalid,
    /// The relay could not be reached, after trying again.
    Unavailable,
}

impl RelayOutcome {
    fn from_status(status: StatusCode) -> Self {
        match status {
            StatusCode::OK => Self::Delivered,
            StatusCode::GONE => Self::Gone,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Self::Stale,
            StatusCode::TOO_MANY_REQUESTS => Self::Limited,
            StatusCode::BAD_REQUEST => Self::Invalid,
            _ => Self::Unavailable,
        }
    }
}

/// `POST /v1/push`, as the relay's `PushRequest` reads it.
#[derive(Serialize)]
struct PushBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    collapse_id: Option<&'a str>,
    handle: &'a str,
    payload: &'a str,
    priority: crate::Priority,
    push_type: crate::PushType,
}

/// The body of a push for `handle`; `None` when its payload is not text.
pub fn push_body(handle: &SecretString, push: &Push) -> Option<Result<Vec<u8>>> {
    let payload = std::str::from_utf8(&push.body).ok()?;
    Some(
        serde_json::to_vec(&PushBody {
            collapse_id: push.collapse_id.as_deref(),
            handle: handle.expose_secret(),
            payload,
            priority: push.priority,
            push_type: push.push_type,
        })
        .map_err(Error::Json),
    )
}

/// Tries `attempt` until it gets an answer other than
/// [`RelayOutcome::Unavailable`], waiting `delays` between tries. `attempt`
/// hears whether this is a retry, which should not reuse the connection.
async fn retrying<F, A>(delays: &[Duration], mut attempt: F) -> RelayOutcome
where
    F: FnMut(bool) -> A,
    A: Future<Output = RelayOutcome>,
{
    let mut outcome = attempt(false).await;
    for delay in delays {
        if outcome != RelayOutcome::Unavailable {
            break;
        }
        tokio::time::sleep(*delay).await;
        outcome = attempt(true).await;
    }
    outcome
}

/// Where a connection goes.
enum Dial {
    Tls {
        host: RelayHost,
        tls: Arc<ClientConfig>,
    },
    /// A test's server on this machine.
    #[cfg(test)]
    Plain(std::net::SocketAddr),
}

pub struct RelayClient {
    connection: Mutex<Option<SendRequest<Full<Bytes>>>>,
    dial: Dial,
}

impl RelayClient {
    pub fn new(host: RelayHost) -> Result<Self> {
        Ok(Self {
            connection: Mutex::new(None),
            dial: Dial::Tls {
                host,
                tls: Arc::new(crate::send::tls()?),
            },
        })
    }

    #[cfg(test)]
    pub(crate) fn plain(address: std::net::SocketAddr) -> Self {
        Self {
            connection: Mutex::new(None),
            dial: Dial::Plain(address),
        }
    }

    /// Sends `push` through the relay to the phone that made `handle`.
    pub async fn post(&self, handle: &SecretString, push: &Push) -> RelayOutcome {
        let Some(Ok(body)) = push_body(handle, push) else {
            return RelayOutcome::Invalid;
        };
        self.call(PUSH, body, &RETRIES).await
    }

    /// Asks the relay to refuse `handle` from now on. Revoking twice is
    /// harmless, so a caller may repeat this until it hears back.
    pub async fn revoke(&self, handle: &SecretString) -> RelayOutcome {
        let Ok(body) = serde_json::to_vec(&serde_json::json!({ "handle": handle.expose_secret() }))
        else {
            return RelayOutcome::Invalid;
        };
        self.call(REVOKE, body, &RETRIES).await
    }

    async fn call(&self, path: &str, body: Vec<u8>, delays: &[Duration]) -> RelayOutcome {
        let body = Bytes::from(body);
        retrying(delays, |fresh| {
            let body = body.clone();
            async move {
                tokio::time::timeout(TIMEOUT, self.once(path, body, fresh))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(RelayOutcome::Unavailable)
            }
        })
        .await
    }

    async fn once(&self, path: &str, body: Bytes, fresh: bool) -> Result<RelayOutcome> {
        let mut sender = self.sender(fresh).await?;
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("https://{}{path}", self.authority()))
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(body))?;
        sender.ready().await?;
        let response = sender.send_request(request).await?;
        Ok(RelayOutcome::from_status(response.status()))
    }

    fn authority(&self) -> &'static str {
        match &self.dial {
            Dial::Tls { host, .. } => host.name(),
            #[cfg(test)]
            Dial::Plain(_) => "relay.test",
        }
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
        match &self.dial {
            Dial::Tls { host, tls } => {
                let name = host.name();
                let server = ServerName::try_from(name).map_err(|_| Error::Host {
                    host: name.to_owned(),
                })?;
                let tcp = TcpStream::connect((name, 443))
                    .await
                    .map_err(Error::Connect)?;
                let stream = TlsConnector::from(tls.clone())
                    .connect(server, tcp)
                    .await
                    .map_err(Error::Connect)?;
                handshake(TokioIo::new(stream)).await
            }
            #[cfg(test)]
            Dial::Plain(address) => {
                let tcp = TcpStream::connect(address).await.map_err(Error::Connect)?;
                handshake(TokioIo::new(tcp)).await
            }
        }
    }
}

async fn handshake<S>(stream: S) -> Result<SendRequest<Full<Bytes>>>
where
    S: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let (sender, connection) = http2::handshake(TokioExecutor::new(), stream).await?;
    // The connection task ends when either side closes it.
    tokio::spawn(connection);
    Ok(sender)
}

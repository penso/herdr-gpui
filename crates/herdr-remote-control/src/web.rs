//! The web app phones use: a static shell (installable as a PWA) that
//! fetches server-rendered fragments and a stream of fragment patches with
//! a bearer token, and a JSON API for commands, pairing, and push.
//!
//! Bearer tokens, not cookies: the phone may reach this computer at its LAN
//! address or its Tailscale address, and the shell installed from one calls
//! the other across origins. CORS admits only this computer's own origins.

mod api;
mod assets;
mod fragments;
mod indicators;
mod log;
mod origins;
mod render;
mod stream;
mod terminal;
mod uploads;

pub(crate) use indicators::{Indicators, Look};
pub(crate) use log::report_limit;
pub(crate) use origins::Origins;
pub(crate) use render::now_ms;
pub(crate) use uploads::Uploads;

use crate::control::DeviceId;
use crate::{
    auth::{self, Auth},
    hub::Hub,
    push::{Privacy, Sender},
    tls::{Authority, Names},
};
use axum::{
    Router,
    extract::{DefaultBodyLimit, FromRequestParts, Request, State as Extract},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock},
};

/// The largest request body: a prompt at most 256 KiB, as JSON.
const BODY_LIMIT: usize = 320 * 1024;

/// What every request reads.
pub(crate) struct State {
    /// Live Activity pushes for the iOS companion, when the user set up an
    /// APNs key.
    pub(crate) apns: Option<crate::apns::Sender>,
    pub(crate) auth: Mutex<Auth>,
    /// Present when the daemon issues its own certificates.
    pub(crate) authority: Option<Authority>,
    pub(crate) computer: String,
    /// Streams open per device.
    pub(crate) connected: Mutex<HashMap<DeviceId, usize>>,
    pub(crate) fingerprint: String,
    pub(crate) hub: Arc<Hub>,
    /// The status marks the lists draw, as Herdr UI last sent them.
    pub(crate) look: Look,
    pub(crate) names: RwLock<Names>,
    pub(crate) port: u16,
    pub(crate) privacy: Privacy,
    pub(crate) push: Option<Sender>,
    /// How often each address may write a phone's error to the log.
    pub(crate) reports: Mutex<auth::RateLimiter<std::net::IpAddr>>,
    /// Origins served before, which an installed app may still send.
    pub(crate) seen: Mutex<Origins>,
    pub(crate) uploads: Uploads,
}

impl State {
    pub(crate) fn auth(&self) -> MutexGuard<'_, Auth> {
        self.auth.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every `https://` origin the server answers at now.
    pub(crate) fn origins(&self) -> Vec<String> {
        self.names
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .origins(self.port)
    }

    /// Whether `origin` may call the API: one served now or before.
    pub(crate) fn admits(&self, origin: &str) -> bool {
        allowed_origin(&self.origins(), origin)
            || self
                .seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(origin)
    }

    pub(crate) fn connected(&self) -> MutexGuard<'_, HashMap<DeviceId, usize>> {
        self.connected
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) type Shared = Arc<State>;

/// Every route, with security headers, CORS, and a body limit.
pub(crate) fn router(state: Shared) -> Router {
    Router::new()
        .route("/", get(assets::shell))
        .route("/board", get(assets::shell))
        .route("/chat/{pane}", get(assets::shell))
        .route("/new", get(assets::shell))
        .route("/new-chat", get(assets::shell))
        .route("/pair", get(assets::shell))
        .route("/settings", get(assets::shell))
        .route("/term/{pane}", get(assets::shell))
        .route("/sw.js", get(assets::service_worker))
        .route("/manifest.webmanifest", get(assets::manifest))
        .route("/apple-touch-icon.png", get(assets::touch_icon))
        .route("/apple-touch-icon-precomposed.png", get(assets::touch_icon))
        .route("/assets/{file}", get(assets::asset))
        .route("/ca.pem", get(assets::ca_pem))
        .route("/ca.mobileconfig", get(assets::mobileconfig))
        .route("/api/ping", get(api::ping))
        .route("/api/pair/claim", post(api::claim))
        .route("/api/pair/claim/{claim}", get(api::claim_status))
        .route("/api/session/challenge", post(api::challenge))
        .route("/api/origins", get(api::origins))
        .route("/api/session", post(api::session))
        .route("/api/handoff", post(api::handoff))
        .route("/api/handoff/redeem", post(api::redeem))
        .route(
            "/api/log",
            post(log::report).layer(DefaultBodyLimit::max(4 * 1024)),
        )
        .route("/api/chat/{pane}/command", post(api::command))
        .route(
            "/api/chat/{pane}/upload",
            post(api::upload).layer(DefaultBodyLimit::max(uploads::MAX_UPLOAD + 1024)),
        )
        .route("/api/workspaces", post(api::create_workspace))
        .route("/api/workspaces/{workspace}/chats", post(api::start_chat))
        .route(
            "/api/push",
            post(api::subscribe_push).delete(api::unsubscribe_push),
        )
        .route("/api/push/key", get(api::push_key))
        .route(
            "/api/apns",
            post(api::register_apns).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/api/term/{pane}/keys", post(terminal::keys))
        .route("/api/term/{pane}/paste", post(terminal::paste))
        .route("/api/term/{pane}/action", post(terminal::action))
        .route("/api/stream", get(stream::stream))
        .route("/f/home", get(fragments::home))
        .route("/f/board", get(fragments::board))
        .route("/f/chat/{pane}", get(fragments::chat))
        .route("/f/chat/{pane}/history", get(fragments::history))
        .route("/f/new", get(fragments::new_workspace))
        .route("/f/new-chat", get(fragments::new_chat))
        .route("/f/settings", get(fragments::settings))
        .route("/f/term/{pane}", get(terminal::fragment))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(middleware::from_fn_with_state(state.clone(), cors))
        .layer(middleware::from_fn(security_headers))
        // Outermost, so refused origins are logged too.
        .layer(middleware::from_fn(log::requests))
        .with_state(state)
}

/// The page's CSP allows only this origin's scripts and styles, pictures
/// from itself and inline `data:` (chat images), and calls to the
/// computer's other origins.
async fn security_headers(request: Request, next: Next) -> Response {
    let api = request.uri().path().starts_with("/api/") || request.uri().path().starts_with("/f/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; connect-src 'self' https:; frame-ancestors 'none'; \
             base-uri 'none'; form-action 'self'; object-src 'none'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    if api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

/// Admits this computer's own origins across addresses; refuses any other
/// origin outright, so no page elsewhere can drive the API.
async fn cors(Extract(state): Extract<Shared>, request: Request, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .map(str::to_owned);
    let Some(origin) = origin else {
        return next.run(request).await;
    };
    if !state.admits(&origin) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let preflight = request.method() == Method::OPTIONS;
    let mut response = if preflight {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    if preflight {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, DELETE"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("authorization, content-type"),
        );
        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
    }
    response
}

pub(crate) fn allowed_origin(origins: &[String], origin: &str) -> bool {
    origins.iter().any(|allowed| allowed == origin)
}

/// The address a connection comes from, for rate limits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Peer(pub(crate) std::net::SocketAddr);

/// The paired device a request's bearer token names.
pub(crate) struct Device(pub(crate) DeviceId);

impl FromRequestParts<Shared> for Device {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, ApiError> {
        let token = bearer(&parts.headers).ok_or(ApiError::Unauthorized)?;
        state
            .auth()
            .authenticate(token, now_ms())
            .map(Device)
            .ok_or(ApiError::Unauthorized)
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// What an API call answers when it fails: a status and a short message
/// the phone shows. Internal causes stay in the daemon.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiError {
    // keep-sorted start
    #[error(transparent)]
    Auth(#[from] auth::Error),
    #[error("that is not a folder you can start a workspace in")]
    Folder,
    #[error(transparent)]
    Hub(#[from] crate::hub::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("notifications are not available")]
    NoPush,
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Push(#[from] crate::push::Error),
    #[error("sign in again")]
    Unauthorized,
    #[error(transparent)]
    Upload(#[from] uploads::Error),
    // keep-sorted end
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Auth(auth::Error::RateLimited) => StatusCode::TOO_MANY_REQUESTS,
            Self::Auth(auth::Error::UnknownDevice) => StatusCode::FORBIDDEN,
            Self::Auth(
                auth::Error::Store { .. }
                | auth::Error::StoreFormat { .. }
                | auth::Error::Random(_),
            ) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Upload(uploads::Error::Write(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Auth(_) | Self::Folder | Self::Invalid(_) | Self::Push(_) | Self::Upload(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::Hub(crate::hub::Error::UnknownChat | crate::hub::Error::UnknownWorkspace) => {
                StatusCode::NOT_FOUND
            }
            Self::Hub(_) | Self::NoPush => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, axum::Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod tests;

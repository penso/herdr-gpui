//! The app shell and its files, compiled in. None carries data: everything
//! about the chats arrives through fragments behind a session.

use super::{ApiError, Shared};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::sync::LazyLock;

/// A compiled-in file: its name, MIME type, and bytes.
struct Asset {
    name: &'static str,
    mime: &'static str,
    bytes: &'static [u8],
}

macro_rules! assets {
    ($($name:literal => $mime:literal),+ $(,)?) => {
        &[$(Asset { name: $name, mime: $mime, bytes: include_bytes!(concat!("assets/", $name)) },)+]
    };
}

const ASSETS: &[Asset] = assets! {
    // keep-sorted start
    "alpine.min.js" => "text/javascript; charset=utf-8",
    "app.css" => "text/css; charset=utf-8",
    "app.js" => "text/javascript; charset=utf-8",
    "icon-180.png" => "image/png",
    "icon-192.png" => "image/png",
    "icon-512.png" => "image/png",
    "icon.svg" => "image/svg+xml",
    "jsqr.min.js" => "text/javascript; charset=utf-8",
    // keep-sorted end
};

const SHELL: &str = include_str!("assets/shell.html");
const TOUCH_ICON: &[u8] = include_bytes!("assets/icon-180.png");
const SERVICE_WORKER: &[u8] = include_bytes!("assets/sw.js");
const MANIFEST: &[u8] = include_bytes!("assets/manifest.webmanifest");

fn file(mime: &'static str, bytes: &'static [u8], cache: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
        ],
        bytes,
    )
        .into_response()
}

/// The shell with its Home Screen icon inline. iOS fetches a linked icon
/// apart from the page and refuses a certificate the phone does not trust
/// yet, even one accepted for the page, then draws a letter instead.
static SHELL_PAGE: LazyLock<String> = LazyLock::new(|| {
    let icon = format!("data:image/png;base64,{}", STANDARD.encode(TOUCH_ICON));
    SHELL.replace("{{touch-icon}}", &icon)
});

/// A page's `?handoff=` token: a paired browser's offer to its Home
/// Screen app, see `auth::handoff`.
#[derive(Deserialize)]
pub(super) struct HandoffQuery {
    handoff: Option<String>,
}

impl HandoffQuery {
    /// The token, when it has only a token's characters; anything else is
    /// dropped rather than echoed.
    fn token(self) -> Option<String> {
        self.handoff.filter(|token| {
            (1..=64).contains(&token.len())
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
    }
}

/// The shell for every route the app shows; it routes on the client. With
/// a handoff token its manifest link carries it, so Safari reads, when the
/// page loads, a manifest whose start address hands the pairing over.
pub(super) async fn shell(Query(query): Query<HandoffQuery>) -> Response {
    let manifest = match query.token() {
        Some(token) => format!("/manifest.webmanifest?handoff={token}"),
        None => "/manifest.webmanifest".to_owned(),
    };
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        SHELL_PAGE.replace("{{manifest}}", &manifest),
    )
        .into_response()
}

/// The icon iOS asks for at the root when a page links none.
pub(super) async fn touch_icon() -> Response {
    file("image/png", TOUCH_ICON, "no-cache")
}

pub(super) async fn service_worker() -> Response {
    let mut response = file("text/javascript; charset=utf-8", SERVICE_WORKER, "no-cache");
    response
        .headers_mut()
        .insert("service-worker-allowed", HeaderValue::from_static("/"));
    response
}

/// The manifest; with a handoff token, its start address carries it.
pub(super) async fn manifest(Query(query): Query<HandoffQuery>) -> Response {
    let Some(token) = query.token() else {
        return file("application/manifest+json", MANIFEST, "no-cache");
    };
    let mut manifest: serde_json::Value = match serde_json::from_slice(MANIFEST) {
        Ok(manifest) => manifest,
        Err(_) => return file("application/manifest+json", MANIFEST, "no-cache"),
    };
    manifest["start_url"] = format!("/?handoff={token}").into();
    (
        [
            (header::CONTENT_TYPE, "application/manifest+json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        manifest.to_string(),
    )
        .into_response()
}

pub(super) async fn asset(Path(name): Path<String>) -> Result<Response, ApiError> {
    let asset = ASSETS
        .iter()
        .find(|asset| asset.name == name)
        .ok_or(ApiError::NotFound)?;
    // The service worker revalidates; a release changes the bytes.
    Ok(file(asset.mime, asset.bytes, "no-cache"))
}

pub(super) async fn ca_pem(State(state): State<Shared>) -> Result<Response, ApiError> {
    let authority = state.authority.as_ref().ok_or(ApiError::NotFound)?;
    let mut response = (
        [(header::CONTENT_TYPE, "application/x-pem-file")],
        authority.ca_pem().to_owned(),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"herdr-remote-ca.pem\""),
    );
    Ok(response)
}

pub(super) async fn mobileconfig(State(state): State<Shared>) -> Result<Response, ApiError> {
    let authority = state.authority.as_ref().ok_or(ApiError::NotFound)?;
    Ok((
        [(header::CONTENT_TYPE, "application/x-apple-aspen-config")],
        authority.mobileconfig(),
    )
        .into_response())
}

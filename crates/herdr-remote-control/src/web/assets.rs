//! The app shell and its files, compiled in. None carries data: everything
//! about the chats arrives through fragments behind a session.

use super::{ApiError, Shared};
use axum::{
    extract::{Path, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

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
    // keep-sorted end
};

const SHELL: &[u8] = include_bytes!("assets/shell.html");
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

/// The shell for every route the app shows; it routes on the client.
pub(super) async fn shell() -> Response {
    file("text/html; charset=utf-8", SHELL, "no-cache")
}

pub(super) async fn service_worker() -> Response {
    let mut response = file("text/javascript; charset=utf-8", SERVICE_WORKER, "no-cache");
    response
        .headers_mut()
        .insert("service-worker-allowed", HeaderValue::from_static("/"));
    response
}

pub(super) async fn manifest() -> Response {
    file("application/manifest+json", MANIFEST, "no-cache")
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

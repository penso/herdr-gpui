//! The daemon log's view of the web app: one line per request, and the
//! errors phones report about themselves, which otherwise stay on a screen
//! nobody can inspect.

use super::{ApiError, Peer, Shared, now_ms};
use crate::auth::RateLimiter;
use axum::{
    Json,
    extract::{ConnectInfo, MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use std::{net::IpAddr, sync::PoisonError, time::Instant};

/// The longest report kept, in characters; the rest is cut.
const MAX_REPORT_CHARS: usize = 600;
/// Reports a phone may send at once, earning one back each refill.
const REPORT_BURST: u32 = 10;
const REPORT_REFILL_MS: u64 = 6_000;
const REPORT_ADDRESSES: usize = 64;

/// Bounds how often each address may write to the log.
pub(crate) fn report_limit() -> RateLimiter<IpAddr> {
    RateLimiter::new(REPORT_BURST, REPORT_REFILL_MS, REPORT_ADDRESSES)
}

/// Logs each request by its route, never its path or query: those carry
/// claim IDs and handoff tokens.
pub(super) async fn requests(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("(no route)", MatchedPath::as_str)
        .to_owned();
    let peer = request
        .extensions()
        .get::<ConnectInfo<Peer>>()
        .map(|ConnectInfo(Peer(peer))| peer.ip());
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let ms = started.elapsed().as_millis();
    match peer {
        Some(peer) => tracing::info!(%peer, %method, %route, status, ms, "request"),
        None => tracing::info!(%method, %route, status, ms, "request"),
    }
    response
}

/// `POST /api/log` `{ context, message }`: an error the phone hit, such as
/// a failed pairing. Unauthenticated, as pairing fails before there is a
/// session, so it is bounded and rate limited per address.
#[derive(Deserialize)]
pub(super) struct Report {
    context: String,
    message: String,
}

pub(super) async fn report(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    Json(report): Json<Report>,
) -> Result<StatusCode, ApiError> {
    let allowed = state
        .reports
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .allow(peer.ip(), now_ms());
    if !allowed {
        return Err(ApiError::Auth(crate::auth::Error::RateLimited));
    }
    // Debug formatting escapes control characters, so a report cannot
    // forge log lines.
    tracing::warn!(
        peer = %peer.ip(),
        context = ?clip(&report.context),
        message = ?clip(&report.message),
        "phone reported an error"
    );
    Ok(StatusCode::NO_CONTENT)
}

fn clip(text: &str) -> &str {
    text.char_indices()
        .nth(MAX_REPORT_CHARS)
        .map_or(text, |(end, _)| &text[..end])
}

#[cfg(test)]
mod tests;

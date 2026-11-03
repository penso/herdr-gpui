//! Nearby pairing over the network (`docs/ios-app-plan.md`, "Nearby
//! pairing, exactly"): a phone that found this computer by Bonjour runs the
//! code exchange here without trusting the certificate yet, since the code
//! both screens show is what authenticates the computer. Open to any
//! origin and rate limited per address, as claiming a pairing is.

use super::{ApiError, Peer, Shared, api::with_auth, now_ms};
use crate::control::{NearbyRevealed, NearbyStarted};
use axum::{
    Json,
    extract::{ConnectInfo, State},
};
use serde::Deserialize;

/// The routes a phone calls before it trusts this computer, with
/// `/api/pair/claim`; CORS admits any origin to them.
pub(super) const START: &str = "/api/nearby/start";
pub(super) const REVEAL: &str = "/api/nearby/reveal";

#[derive(Deserialize)]
pub(super) struct StartRequest {
    public_key: String,
}

/// `POST /api/nearby/start`: refused (409) unless a pairing dialog is open.
pub(super) async fn start(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    Json(request): Json<StartRequest>,
) -> Result<Json<NearbyStarted>, ApiError> {
    let shared = state.clone();
    let started = with_auth(&state, move |auth| {
        Ok(auth.nearby_start(
            Some(peer.ip()),
            &request.public_key,
            &shared.computer,
            &shared.fingerprint,
            now_ms(),
        )?)
    })
    .await?;
    Ok(Json(started))
}

#[derive(Deserialize)]
pub(super) struct RevealRequest {
    session: String,
    nonce: String,
}

/// `POST /api/nearby/reveal`: the daemon's nonce and the sealed token;
/// 404 when the session is unknown or expired.
pub(super) async fn reveal(
    State(state): State<Shared>,
    Json(request): Json<RevealRequest>,
) -> Result<Json<NearbyRevealed>, ApiError> {
    let shared = state.clone();
    let revealed = with_auth(&state, move |auth| {
        let origins = shared.origins();
        Ok(auth.nearby_reveal(
            &request.session,
            &request.nonce,
            &shared.fingerprint,
            &origins,
            now_ms(),
        )?)
    })
    .await?;
    Ok(Json(revealed))
}

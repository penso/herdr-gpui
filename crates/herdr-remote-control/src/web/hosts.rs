//! The other hosts a phone may be paired with from this computer
//! (`control::hosts`): `GET /api/v2/hosts` lists them with where the phone
//! stands with each, and `POST /api/v2/hosts/{host}/pair` hands over the
//! key the phone made for one, which Herdr UI enrolls on that host. Both
//! need the phone's session; a phone only ever sees its own.

use super::{ApiError, Device, Shared, api::with_auth, render::now_ms};
use crate::control::hosts::{HostId, HostLink, HostState};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(super) struct Hosts {
    hosts: Vec<HostLink>,
}

pub(super) async fn list(
    State(state): State<Shared>,
    Device(device): Device,
) -> Result<Json<Hosts>, ApiError> {
    let hosts = with_auth(&state, move |auth| Ok(auth.host_links(&device, now_ms())?)).await?;
    Ok(Json(Hosts { hosts }))
}

#[derive(Deserialize)]
pub(super) struct PairRequest {
    public_key: String,
}

pub(super) async fn pair(
    State(state): State<Shared>,
    Device(device): Device,
    Path(host): Path<String>,
    Json(request): Json<PairRequest>,
) -> Result<Json<HostState>, ApiError> {
    let state = with_auth(&state, move |auth| {
        Ok(auth.ask_host(&device, &HostId(host), &request.public_key, now_ms())?)
    })
    .await?;
    Ok(Json(state))
}

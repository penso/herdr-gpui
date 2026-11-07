//! The other hosts a phone may be paired with from this computer
//! (`control::hosts`): `GET /api/v2/hosts` lists them with where the phone
//! stands with each, and `POST /api/v2/hosts/{host}/pair` hands over the
//! key the phone made for one, which Sheprd enrolls on that host. Both
//! need the phone's session; a phone only ever sees its own. A web app's
//! origin goes along, as the host's CORS must admit the page holding the key.

use super::{
    ApiError, Device, Shared,
    api::{app_origin, with_auth},
    render::now_ms,
};
use crate::control::hosts::{HostId, HostLink, HostState};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
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
    headers: HeaderMap,
    Json(request): Json<PairRequest>,
) -> Result<Json<HostState>, ApiError> {
    // The page holding the key calls from this origin, and the host must
    // admit it. Unlike a claim, this computer's own origin counts: to the
    // host it is another computer's. The iOS app sends none.
    let app = headers
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .map(|origin| app_origin(origin).ok_or(ApiError::Invalid("the app's origin is not valid")))
        .transpose()?;
    let state = with_auth(&state, move |auth| {
        Ok(auth.ask_host(&device, &HostId(host), &request.public_key, app, now_ms())?)
    })
    .await?;
    Ok(Json(state))
}

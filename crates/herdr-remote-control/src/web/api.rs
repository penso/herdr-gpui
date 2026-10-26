//! The JSON API: pairing, sessions, chat commands, new workspaces and
//! chats, and push subscriptions. Auth calls that write to disk run on the
//! blocking pool.

use super::{
    ApiError, Device, Peer, Shared,
    fragments::folders,
    render::{now_ms, urlencode},
};
use crate::{
    auth::{ChallengeId, ClaimId},
    control::{DeviceId, PairingState},
    push::Subscription,
};
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, header},
};
use herdr_chat_protocol::{Agent, Command};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

/// How long starting a chat waits for it to open before answering with the
/// list instead.
const NEW_CHAT_WAIT: Duration = Duration::from_secs(10);

/// Runs `action` with the auth state on the blocking pool.
async fn with_auth<T: Send + 'static>(
    state: &Shared,
    action: impl FnOnce(&mut crate::auth::Auth) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || action(&mut state.auth()))
        .await
        .map_err(|_| ApiError::Invalid("the request was interrupted"))?
}

/// Says which computer answers, by its certificate's fingerprint: an
/// address a phone saved may belong to another computer on another network.
pub(super) async fn ping(State(state): State<Shared>) -> Json<Value> {
    Json(json!({ "ok": true, "computer": state.computer, "fingerprint": state.fingerprint }))
}

#[derive(Deserialize)]
pub(super) struct ClaimRequest {
    token: String,
    name: String,
    public_key: String,
}

/// A claim from an app installed from another computer carries that app's
/// origin, which approving the claim admits; a claim from this computer's
/// own page carries none.
pub(super) async fn claim(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    headers: HeaderMap,
    Json(request): Json<ClaimRequest>,
) -> Result<Json<Value>, ApiError> {
    let app = headers
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        // Every device paired from another computer's app records that
        // origin, even one already admitted for another device, so revoking
        // one leaves the others working.
        .filter(|origin| !state.serves(origin))
        .map(|origin| app_origin(origin).ok_or(ApiError::Invalid("the app's origin is not valid")))
        .transpose()?;
    let claim = with_auth(&state, move |auth| {
        let claim = auth.claim(
            peer.ip(),
            &request.token,
            &request.name,
            &request.public_key,
            now_ms(),
        )?;
        if let Some(app) = app {
            auth.claimed_from(&claim.id, app);
        }
        Ok(claim)
    })
    .await?;
    Ok(Json(json!({
        "claim": claim.id.0,
        "code": claim.code,
        "expires_ms": claim.expires_ms,
    })))
}

/// `origin` when it is an `https://` origin as a browser sends it: a host
/// and an optional port, nothing more, at most 255 bytes.
pub(super) fn app_origin(origin: &str) -> Option<String> {
    let host = origin.strip_prefix("https://")?;
    let valid = !host.is_empty()
        && origin.len() <= 255
        && host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        });
    valid.then(|| origin.to_owned())
}

pub(super) async fn claim_status(
    State(state): State<Shared>,
    Path(claim): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let pairing = with_auth(&state, move |auth| {
        Ok(auth.claim_status(&ClaimId(claim), now_ms())?)
    })
    .await?;
    Ok(Json(match pairing {
        PairingState::Approved { device } => json!({
            "state": "approved",
            "device": device.0,
            "origins": state.origins(),
            "fingerprint": state.fingerprint,
            "computer": state.computer,
        }),
        PairingState::Claimed { .. } | PairingState::Waiting { .. } => json!({"state": "claimed"}),
        PairingState::Denied => json!({"state": "denied"}),
        PairingState::Expired => json!({"state": "expired"}),
    }))
}

/// A token the paired browser puts in its Home Screen app's start address.
pub(super) async fn handoff(
    State(state): State<Shared>,
    Device(device): Device,
) -> Result<Json<Value>, ApiError> {
    let handoff = with_auth(&state, move |auth| Ok(auth.handoff(&device, now_ms())?)).await?;
    Ok(Json(json!({
        "token": handoff.token,
        "expires_ms": handoff.expires_ms,
    })))
}

#[derive(Deserialize)]
pub(super) struct RedeemRequest {
    token: String,
    public_key: String,
}

/// The Home Screen app takes over its browser's device with its own key;
/// it answers as an approved claim does.
pub(super) async fn redeem(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    Json(request): Json<RedeemRequest>,
) -> Result<Json<Value>, ApiError> {
    let device = with_auth(&state, move |auth| {
        Ok(auth.redeem(peer.ip(), &request.token, &request.public_key, now_ms())?)
    })
    .await?;
    Ok(Json(json!({
        "device": device.0,
        "origins": state.origins(),
        "fingerprint": state.fingerprint,
        "computer": state.computer,
    })))
}

#[derive(Deserialize)]
pub(super) struct ChallengeRequest {
    device: String,
}

pub(super) async fn challenge(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    Json(request): Json<ChallengeRequest>,
) -> Result<Json<Value>, ApiError> {
    let challenge = with_auth(&state, move |auth| {
        Ok(auth.challenge(peer.ip(), &DeviceId(request.device), now_ms())?)
    })
    .await?;
    Ok(Json(json!({
        "challenge": challenge.id.0,
        "nonce": challenge.nonce,
        "expires_ms": challenge.expires_ms,
    })))
}

#[derive(Deserialize)]
pub(super) struct SessionRequest {
    challenge: String,
    signature: String,
}

pub(super) async fn session(
    State(state): State<Shared>,
    Json(request): Json<SessionRequest>,
) -> Result<Json<Value>, ApiError> {
    let session = with_auth(&state, move |auth| {
        Ok(auth.verify(
            &ChallengeId(request.challenge),
            &request.signature,
            now_ms(),
        )?)
    })
    .await?;
    // The addresses served now, so an installed app that paired on another
    // network can still find this computer later.
    Ok(Json(json!({
        "token": session.token,
        "expires_ms": session.expires_ms,
        "origins": state.origins(),
    })))
}

/// The addresses served now, for a phone that signed in earlier: networks
/// change while a session lasts, and its saved list must follow them.
pub(super) async fn origins(State(state): State<Shared>, _device: Device) -> Json<Value> {
    Json(json!({ "origins": state.origins() }))
}

pub(super) async fn command(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(command): Json<Command>,
) -> Result<Json<Value>, ApiError> {
    // A phone names only pictures it uploaded, never a path here.
    let command = match command {
        Command::Prompt { text, attachments } => Command::Prompt {
            text,
            attachments: state.uploads.resolve(&attachments, now_ms())?,
        },
        command => command,
    };
    let rejected = state.hub.command(&pane, command).await?;
    Ok(Json(
        json!({ "rejected": rejected.map(|rejection| rejection.to_string()) }),
    ))
}

pub(super) async fn upload(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    bytes: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    state.hub.chat(&pane).ok_or(ApiError::NotFound)?;
    let id = crate::auth::random_id()?;
    let stored = state.clone();
    let id = tokio::task::spawn_blocking(move || stored.uploads.store(id, &bytes, now_ms()))
        .await
        .map_err(|_| ApiError::Invalid("the upload was interrupted"))??;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub(super) struct NewWorkspace {
    cwd: String,
    #[serde(default)]
    branch: String,
    agent: String,
}

pub(super) async fn create_workspace(
    State(state): State<Shared>,
    _device: Device,
    Json(request): Json<NewWorkspace>,
) -> Result<Json<Value>, ApiError> {
    let agent: Agent = request
        .agent
        .parse()
        .map_err(|_| ApiError::Invalid("unknown agent"))?;
    // Only a folder the form offered: never a path the phone made up.
    let layout = state.hub.layout();
    let offered = tokio::task::spawn_blocking(move || folders(&layout))
        .await
        .map_err(|_| ApiError::Folder)?;
    if !offered.offers(&request.cwd) {
        return Err(ApiError::Folder);
    }
    let branch = request.branch.trim();
    if !branch.is_empty() && !valid_branch(branch) {
        return Err(ApiError::Invalid("that is not a valid branch name"));
    }
    state
        .hub
        .create_workspace(&request.cwd, (!branch.is_empty()).then_some(branch), agent)
        .await?;
    Ok(Json(json!({ "location": "/" })))
}

/// A conservative subset of Git's branch name rules.
pub(crate) fn valid_branch(name: &str) -> bool {
    name.len() <= 200
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}

#[derive(Deserialize)]
pub(super) struct NewChat {
    agent: String,
}

pub(super) async fn start_chat(
    State(state): State<Shared>,
    _device: Device,
    Path(workspace): Path<String>,
    Json(request): Json<NewChat>,
) -> Result<Json<Value>, ApiError> {
    let agent: Agent = request
        .agent
        .parse()
        .map_err(|_| ApiError::Invalid("unknown agent"))?;
    let pane = state.hub.start_chat(&workspace, agent, None).await?;
    // The chat is listed once its program reports in; open it then, or
    // the list when it is slow to start.
    let location = if state.hub.chat_ready(&pane, NEW_CHAT_WAIT).await {
        format!("/chat/{}", urlencode(&pane))
    } else {
        "/".to_owned()
    };
    Ok(Json(json!({ "location": location })))
}

/// The iOS companion's Live Activity tokens: the push-to-start token when
/// it changed, and every activity it still shows.
pub(super) async fn register_apns(
    State(state): State<Shared>,
    Device(device): Device,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    if state.apns.is_none() {
        return Err(ApiError::NoPush);
    }
    let update = crate::apns::Registration::parse(&body)
        .map_err(|_| ApiError::Invalid("that registration is not valid"))?;
    with_auth(&state, move |auth| {
        let devices = auth.devices_mut();
        let mut stored = crate::apns::Registration::from_stored(
            devices.get(&device).and_then(|record| record.apns.as_ref()),
        );
        stored.merge(update);
        let value = stored
            .to_stored()
            .map_err(|_| ApiError::Invalid("that registration cannot be stored"))?;
        Ok(devices.set_apns(&device, value)?)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn push_key(
    State(state): State<Shared>,
    _device: Device,
) -> Result<Json<Value>, ApiError> {
    let sender = state.push.as_ref().ok_or(ApiError::NoPush)?;
    Ok(Json(json!({ "key": sender.vapid().public_key_b64url() })))
}

pub(super) async fn subscribe_push(
    State(state): State<Shared>,
    Device(device): Device,
    Json(subscription): Json<Subscription>,
) -> Result<Json<Value>, ApiError> {
    if state.push.is_none() {
        return Err(ApiError::NoPush);
    }
    // Deserializing validated it: an allowed push service, sound keys.
    let value = serde_json::to_value(&subscription)
        .map_err(|_| ApiError::Invalid("that subscription cannot be stored"))?;
    with_auth(&state, move |auth| {
        Ok(auth.devices_mut().set_push(&device, Some(value))?)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn unsubscribe_push(
    State(state): State<Shared>,
    Device(device): Device,
) -> Result<Json<Value>, ApiError> {
    with_auth(&state, move |auth| {
        Ok(auth.devices_mut().set_push(&device, None)?)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

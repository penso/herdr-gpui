//! The API the native iOS app reads (`docs/ios-app-plan.md`, "The v2 wire,
//! exactly"): home and a chat as JSON, and one WebSocket per phone that
//! follows the views it subscribes to. Commands keep the web app's `POST`
//! routes, so one rate limit and `Origin` policy covers every write.

mod create;
mod live;
mod model;
mod search;
mod wire;

pub(crate) use create::{create_workspace, new_options, start_chat};
pub(crate) use search::search;

use crate::{
    control::DeviceId,
    hub::Summary,
    web::{ApiError, Connected, Device, Shared, bearer},
};
use axum::{
    Json,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::Response,
};
use model::{ChatSnapshot, Home, Sources};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc;
use wire::{ClientMessage, ServerMessage};

/// The largest message a phone may send; a bigger one closes the socket.
const MAX_INBOUND: usize = 4 * 1024;
/// Messages queued for a slow phone; a chat that falls further behind its
/// own queue is told to reload.
const OUTBOUND_QUEUE: usize = 64;
/// Messages from the phone waiting to be handled.
const INBOUND_QUEUE: usize = 16;
/// Streams and sockets one device may hold open at once.
const MAX_STREAMS: usize = 8;
/// How often an idle socket is pinged, so proxies and NATs keep it.
const PING_INTERVAL: Duration = Duration::from_secs(20);

/// This computer's home as the state holds it now.
fn home(state: &Shared) -> Home {
    let layout = state.hub.layout();
    let summaries: HashMap<String, Summary> = layout
        .chats()
        .filter_map(|chat| {
            state
                .hub
                .chat(&chat.pane)
                .map(|link| (chat.pane.clone(), link.summary()))
        })
        .collect();
    let pull_requests = state.pull_requests.borrow().clone();
    Home::project(Sources {
        computer: &state.computer,
        fingerprint: &state.fingerprint,
        layout: &layout,
        load: *state.load.borrow(),
        summaries,
        pull_requests: &pull_requests,
    })
}

/// `GET /api/v2/home`.
pub(crate) async fn get_home(State(state): State<Shared>, _device: Device) -> Json<Home> {
    Json(home(&state))
}

/// `GET /api/v2/chat/{pane}`: the transcript and its cursor.
pub(crate) async fn get_chat(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
) -> Result<Json<ChatSnapshot>, ApiError> {
    state.hub.layout().chat(&pane).ok_or(ApiError::NotFound)?;
    let link = state.hub.chat(&pane).ok_or(ApiError::NotFound)?;
    let (transcript, cursor) = link.base();
    Ok(Json(ChatSnapshot {
        pane,
        cursor,
        connected: link.summary().connected,
        transcript,
    }))
}

/// `GET /api/v2/live`: upgrades to the socket, unless the device holds
/// [`MAX_STREAMS`] open already.
pub(crate) async fn live(
    State(state): State<Shared>,
    Device(device): Device,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let open = state.connected().get(&device).copied().unwrap_or(0);
    if open >= MAX_STREAMS {
        return Err(ApiError::TooManyStreams);
    }
    // `Device` accepted this token, so it is present.
    let token = bearer(&headers).unwrap_or_default().to_owned();
    Ok(upgrade
        .max_message_size(MAX_INBOUND)
        .max_frame_size(MAX_INBOUND)
        .on_upgrade(move |socket| serve(socket, state, device, token)))
}

/// Carries messages between the socket and the session until either ends:
/// the phone closes, sends a binary or oversized frame, or the session
/// says `bye`.
async fn serve(mut socket: WebSocket, state: Shared, device: DeviceId, token: String) {
    let _connected = Connected::new(state.clone(), device.clone());
    let (to_phone, mut outbound) = mpsc::channel(OUTBOUND_QUEUE);
    let (inbound, from_phone) = mpsc::channel(INBOUND_QUEUE);
    let session = tokio::spawn(live::run(state, token, device, from_phone, to_phone));
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.reset();
    loop {
        tokio::select! {
            message = outbound.recv() => {
                let Some(message) = message else { break };
                let bye = matches!(message, ServerMessage::Bye { .. });
                if !send(&mut socket, &message).await || bye {
                    break;
                }
            }
            frame = socket.recv() => match frame {
                Some(Ok(Message::Text(text))) => {
                    let reply = match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(message) => inbound
                            .try_send(message)
                            .err()
                            .map(|_| "too many messages at once"),
                        Err(_) => Some("that message is not one this socket reads"),
                    };
                    if let Some(message) = reply {
                        let error = ServerMessage::Error { message: message.to_owned() };
                        if !send(&mut socket, &error).await {
                            break;
                        }
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Binary(_) | Message::Close(_)) | Err(_)) | None => break,
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }
    session.abort();
    let _ = socket.send(Message::Close(None)).await;
}

async fn send(socket: &mut WebSocket, message: &ServerMessage) -> bool {
    let Ok(text) = serde_json::to_string(message) else {
        return false;
    };
    socket.send(Message::Text(text.into())).await.is_ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

//! The patch stream: server-sent events that replace regions of the view
//! the phone shows as the chats change. The first event brings the view up
//! to date, so a fragment fetched just before the stream opened cannot miss
//! a change. Bursts coalesce, a lagging stream reloads its view, and the
//! stream counts as the device being connected.

use super::{Device, Shared, bearer, fragments::lists, render};
use crate::{control::DeviceId, hub::Event};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event as Sse, KeepAlive, Sse as Stream},
};
use futures_lite::stream;
use serde::Deserialize;
use serde_json::json;
use std::{convert::Infallible, time::Duration};
use tokio::sync::{broadcast::error::RecvError, mpsc};

/// How long a chat's changes gather before one patch goes out.
const CHAT_COALESCE: Duration = Duration::from_millis(120);
/// How long list changes gather before one patch goes out.
const LIST_COALESCE: Duration = Duration::from_millis(400);
/// How often a terminal's screen is read while a phone shows it.
const SCREEN_INTERVAL: Duration = Duration::from_millis(300);
/// Patches queued for a slow phone before the stream gives up.
const QUEUE: usize = 32;
/// How often an open stream checks that its session still holds, so one
/// that expires ends within this long. A revocation ends it at once.
const ACCESS_CHECK: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
pub(super) struct Target {
    view: String,
    #[serde(default)]
    pane: Option<String>,
}

/// One open stream, counted against its device until it is dropped.
struct Connected {
    state: Shared,
    device: DeviceId,
}

impl Connected {
    fn new(state: Shared, device: DeviceId) -> Self {
        *state.connected().entry(device.clone()).or_default() += 1;
        Self { state, device }
    }
}

impl Drop for Connected {
    fn drop(&mut self) {
        let mut connected = self.state.connected();
        if let Some(count) = connected.get_mut(&self.device) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                connected.remove(&self.device);
            }
        }
    }
}

pub(super) async fn stream(
    State(state): State<Shared>,
    Device(device): Device,
    headers: HeaderMap,
    Query(target): Query<Target>,
) -> Stream<impl futures_lite::Stream<Item = Result<Sse, Infallible>>> {
    let (sender, receiver) = mpsc::channel(QUEUE);
    // `Device` accepted this token, so it is present.
    let token = bearer(&headers).unwrap_or_default().to_owned();
    let guard = Connected::new(state.clone(), device);
    tokio::spawn(async move {
        let _guard = guard;
        let follow = async {
            match (target.view.as_str(), target.pane) {
                ("chat", Some(pane)) => follow_chat(&state, &pane, &sender).await,
                ("term", Some(pane)) => follow_terminal(&state, &pane, &sender).await,
                ("board", _) => follow_lists(&state, true, &sender).await,
                _ => follow_lists(&state, false, &sender).await,
            }
        };
        tokio::select! {
            () = follow => {}
            () = lose_access(&state, &token) => {
                // Reloading fetches fragments, which ask the phone to sign in.
                let _ = sender.send(reload()).await;
            }
        }
    });
    let events = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|event| (Ok(event), receiver))
    });
    Stream::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}

/// Returns once the session `token` no longer authenticates: its device
/// was revoked or forgotten, or the session expired.
async fn lose_access(state: &Shared, token: &str) {
    let mut revocations = state.auth().watch_revocations();
    let mut check = tokio::time::interval(ACCESS_CHECK);
    loop {
        tokio::select! {
            _ = check.tick() => {}
            changed = revocations.changed() => if changed.is_err() { return },
        }
        if state.auth().authenticate(token, render::now_ms()).is_none() {
            return;
        }
    }
}

fn reload() -> Sse {
    Sse::default().event("reload").data("{}")
}

fn patch(regions: Vec<(&str, String)>, badge: Option<usize>) -> Sse {
    let regions: serde_json::Map<String, serde_json::Value> = regions
        .into_iter()
        .map(|(id, html)| (id.to_owned(), json!(html)))
        .collect();
    let mut body = json!({ "regions": regions });
    if let Some(badge) = badge {
        body["badge"] = json!(badge);
    }
    Sse::default().event("patch").data(body.to_string())
}

/// Sends the chat's regions now and after every burst of changes, until
/// the phone goes away or the chat does.
async fn follow_chat(state: &Shared, pane: &str, sender: &mpsc::Sender<Sse>) {
    let Some(link) = state.hub.chat(pane) else {
        let _ = sender.send(reload()).await;
        return;
    };
    let (_, mut events) = link.subscribe();
    let mut layout = state.hub.watch_layout();
    loop {
        let current = state.hub.layout();
        let Some((workspace, row)) = current.chat(pane) else {
            let _ = sender.send(reload()).await;
            return;
        };
        let regions = render::chat_patches(workspace, row, &link.view(), render::now_ms())
            .into_iter()
            .map(|(id, markup)| (id, markup.into_string()))
            .collect();
        if sender.send(patch(regions, None)).await.is_err() {
            return;
        }
        tokio::select! {
            event = events.recv() => match event {
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => {
                    let _ = sender.send(reload()).await;
                    return;
                }
            },
            changed = layout.changed() => if changed.is_err() { return },
            () = sender.closed() => return,
        }
        tokio::time::sleep(CHAT_COALESCE).await;
        // Whatever else arrived meanwhile is in the next render.
        while events.try_recv().is_ok() {}
    }
}

/// Sends the pane's screen whenever it changes, read every
/// [`SCREEN_INTERVAL`]: Herdr's API has no output events. Reading stops
/// with the stream, so an idle phone costs nothing.
async fn follow_terminal(state: &Shared, pane: &str, sender: &mpsc::Sender<Sse>) {
    let mut last = None;
    let mut failures = 0;
    loop {
        if state.hub.layout().terminal(pane).is_none() {
            let _ = sender.send(reload()).await;
            return;
        }
        match super::terminal::read_screen(state, pane).await {
            Ok(text) => {
                failures = 0;
                if last.as_ref() != Some(&text) {
                    let screen = render::screen(&text);
                    let markup = render::terminal_screen(&screen).into_string();
                    if sender
                        .send(patch(vec![("term-screen", markup)], None))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    last = Some(text);
                }
            }
            // A pane that closed, or Herdr gone: the view reloads to say so.
            Err(_) => {
                failures += 1;
                if failures >= 3 {
                    let _ = sender.send(reload()).await;
                    return;
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep(SCREEN_INTERVAL) => {}
            () = sender.closed() => return,
        }
    }
}

/// Sends home or the board now and after every burst of changes in any
/// chat or in the layout.
async fn follow_lists(state: &Shared, board: bool, sender: &mpsc::Sender<Sse>) {
    let mut all = state.hub.subscribe_all();
    let mut layout = state.hub.watch_layout();
    loop {
        let (markup, waiting) = lists(state, board);
        if sender
            .send(patch(vec![("lists", markup.into_string())], Some(waiting)))
            .await
            .is_err()
        {
            return;
        }
        loop {
            tokio::select! {
                event = all.recv() => match event {
                    // Streamed text alone does not change a list.
                    Ok((_, Event::Change { change, .. })) if matches!(*change, herdr_chat_protocol::Change::Append { .. }) => continue,
                    Ok(_) | Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return,
                },
                changed = layout.changed() => if changed.is_err() { return } else { break },
                () = sender.closed() => return,
            }
        }
        tokio::time::sleep(LIST_COALESCE).await;
        while all.try_recv().is_ok() {}
    }
}

//! Live Activities for the iOS companion: each chat change the APNs rules
//! call for is paced per chat, then sent to every device that registered
//! tokens: the push-to-start token to start an activity, the activity's own
//! token to update or end it. A token APNs calls invalid is forgotten.

use super::notifier::waiting;
use crate::{
    apns::{DeviceToken, Event, Live, Outcome, Pacer, Registration, decide},
    control::DeviceId,
    push::Chat,
    web::{State, now_ms},
};
use herdr_chat_protocol::{Change, Status, Transcript};
use std::{collections::HashMap, sync::Arc};

/// How long an update waits for the activity's own token, which the phone
/// registers after the activity starts.
const HOLD_MS: u64 = 10 * 60 * 1000;

#[derive(Default)]
pub(super) struct LiveActivities {
    pacer: Pacer,
    /// The latest update or end for an activity whose token the device has
    /// not registered yet, by device and pane. A short turn can end before
    /// the token arrives; its end must still reach the phone.
    held: HashMap<(DeviceId, String), Held>,
}

struct Held {
    live: Live,
    since_ms: u64,
}

impl LiveActivities {
    /// Considers `change` to the chat in `pane`, whose transcript was
    /// `before`.
    pub(super) fn change(
        &mut self,
        state: &Arc<State>,
        pane: &str,
        before: &Transcript,
        change: &Change,
    ) {
        if state.apns.is_none() {
            return;
        }
        let layout = state.hub.layout();
        let Some((workspace, row)) = layout.chat(pane) else {
            return;
        };
        let agent = if before.agent.is_empty() {
            row.agent.as_deref().unwrap_or("The agent")
        } else {
            &before.agent
        };
        let chat = Chat {
            agent,
            pane,
            title: &row.title,
            workspace: &workspace.label,
        };
        let now = now_ms();
        let Some(live) = decide(
            &chat,
            &state.computer,
            &state.fingerprint,
            before,
            change,
            now,
        ) else {
            return;
        };
        if let Some(live) = self.pacer.offer(live, now) {
            self.dispatch(state, live);
        }
    }

    /// Sends the updates the pacer held back whose time came.
    pub(super) fn tick(&mut self, state: &Arc<State>) {
        if state.apns.is_none() {
            return;
        }
        for live in self.pacer.due(now_ms()) {
            self.dispatch(state, live);
        }
        self.release(state);
    }

    /// Sends `live` to every device with a token for it, and holds it for
    /// devices whose activity token has not arrived.
    fn dispatch(&mut self, state: &Arc<State>, mut live: Live) {
        live.state.waiting = waiting(state);
        live.state.working = working(state);
        let now = now_ms();
        for (device, registration) in registrations(state) {
            let key = (device, live.pane.clone());
            // Whatever was held is older than this, or was for the activity
            // a start replaces.
            self.held.remove(&key);
            match route(&live, &registration) {
                Route::Send(token) => send(state, key.0, token.clone(), live.clone()),
                Route::Hold => {
                    self.held.insert(
                        key,
                        Held {
                            live: live.clone(),
                            since_ms: now,
                        },
                    );
                }
                Route::Skip => {}
            }
        }
    }

    /// Sends what was held to the devices whose activity token arrived, and
    /// drops what waited too long.
    fn release(&mut self, state: &Arc<State>) {
        if self.held.is_empty() {
            return;
        }
        let now = now_ms();
        let registrations = registrations(state);
        let (waiting, working) = (waiting(state), working(state));
        self.held.retain(|(device, pane), held| {
            if now.saturating_sub(held.since_ms) > HOLD_MS {
                return false;
            }
            let token = registrations
                .iter()
                .find(|(registered, _)| registered == device)
                .and_then(|(_, registration)| registration.activity(pane));
            let Some(token) = token else {
                return true;
            };
            let mut live = held.live.clone();
            live.state.waiting = waiting;
            live.state.working = working;
            send(state, device.clone(), token.clone(), live);
            false
        });
    }
}

/// What to do with an update for one device.
#[derive(Debug, Eq, PartialEq)]
enum Route<'a> {
    Send(&'a DeviceToken),
    /// Wait for the activity's own token.
    Hold,
    Skip,
}

/// A start goes to the push-to-start token; an update or end to the
/// activity's token, or waits for it on a device that can start
/// activities, which may be showing one whose token is on its way.
fn route<'a>(live: &Live, registration: &'a Registration) -> Route<'a> {
    let token = match live.event {
        Event::Start { .. } => registration.push_to_start.as_ref(),
        _ => registration.activity(&live.pane),
    };
    match token {
        Some(token) => Route::Send(token),
        None if registration.push_to_start.is_some()
            && !matches!(live.event, Event::Start { .. }) =>
        {
            Route::Hold
        }
        None => Route::Skip,
    }
}

/// Chats with a turn running now, for the Dynamic Island's count.
fn working(state: &State) -> u32 {
    let layout = state.hub.layout();
    let count = layout
        .chats()
        .filter_map(|chat| state.hub.chat(&chat.pane))
        .filter(|link| link.summary().status == Status::Working)
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Every device's non-empty APNs registration, read under one lock: the
/// auth mutex is not reentrant.
pub(super) fn registrations(state: &State) -> Vec<(DeviceId, Registration)> {
    let auth = state.auth();
    let devices = auth.devices();
    devices
        .list(|_| false)
        .into_iter()
        .filter_map(|device| {
            let registration = Registration::from_stored(
                devices
                    .get(&device.id)
                    .and_then(|record| record.apns.as_ref()),
            );
            (!registration.is_empty()).then_some((device.id, registration))
        })
        .collect()
}

/// Sends `live` to `device` with `token`; forgets a token APNs calls
/// invalid, and an activity that ended.
fn send(state: &Arc<State>, device: DeviceId, token: DeviceToken, live: Live) {
    let state = state.clone();
    tokio::spawn(async move {
        let Some(sender) = &state.apns else {
            return;
        };
        let outcome = sender.send(&token, &live, now_ms() / 1000).await;
        let ended = matches!(live.event, Event::End { .. });
        if !matches!(outcome, Ok(Outcome::Gone)) && !ended {
            return;
        }
        let gone = matches!(outcome, Ok(Outcome::Gone));
        let pane = live.pane.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut auth = state.auth();
            let devices = auth.devices_mut();
            let mut registration = Registration::from_stored(
                devices.get(&device).and_then(|record| record.apns.as_ref()),
            );
            let changed = if gone {
                registration.forget(&token)
            } else {
                registration.end(&pane)
            };
            if changed && let Ok(value) = registration.to_stored() {
                let _ = devices.set_apns(&device, value);
            }
        })
        .await;
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

//! Turns chat changes into push notifications for every device that asked
//! for them. It keeps its own copy of each transcript, stepped with the
//! shared reducer, so a trigger sees the state before each change. A
//! device the native app registered for alerts gets them sealed through
//! APNs; any other gets Web Push, which is blocking HTTPS on the blocking
//! pool. A push service that says a token or subscription is gone drops it
//! from the device. While Herdr UI marks a device near (presence), its
//! alerts are held and go out when the owner leaves.

use crate::{
    apns::{self, Registration, alert::Sealed},
    control::DeviceId,
    hub::Event,
    push::{Chat, Delivery, Hold, Notification, Outcome, Subscription, Throttle, decide},
    web::{State, now_ms},
};
use herdr_chat_protocol::{Cursor, Transcript};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{broadcast::error::RecvError, mpsc};

/// How often held-back notifications are checked.
const TICK: Duration = Duration::from_secs(1);

pub(super) async fn run(state: Arc<State>) {
    if state.push.is_none() && state.apns.is_none() {
        return;
    }
    let mut live = super::live::LiveActivities::default();
    let mut events = state.hub.subscribe_all();
    let mut transcripts: HashMap<String, Base> = HashMap::new();
    let mut throttles: HashMap<DeviceId, Throttle> = HashMap::new();
    let mut outbox = Outbox::default();
    let mut holds: HashMap<DeviceId, Hold> = HashMap::new();
    let mut ticker = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok((pane, Event::Reset { transcript, cursor })) => {
                    transcripts.insert(pane, Base {
                        transcript: Transcript::clone(&transcript),
                        cursor: Some(cursor),
                    });
                }
                Ok((pane, Event::Change { change, cursor })) => {
                    if !transcripts.contains_key(&pane) {
                        let Some(link) = state.hub.chat(&pane) else {
                            continue;
                        };
                        let (transcript, cursor) = link.base();
                        transcripts.insert(pane.clone(), Base { transcript, cursor });
                    }
                    let Some(base) = transcripts.get_mut(&pane) else {
                        continue;
                    };
                    // The copy already holds this change.
                    if base.includes(cursor) {
                        continue;
                    }
                    let before = &mut base.transcript;
                    let notification = notification(&state, &pane, before, &change);
                    live.change(&state, &pane, before, &change);
                    if before.apply(&change).is_err() {
                        // Rebuilt from the next Reset or change.
                        transcripts.remove(&pane);
                    } else {
                        base.cursor = Some(cursor);
                    }
                    if let Some(notification) = notification {
                        offer(&state, &mut throttles, &mut holds, &mut outbox, notification);
                    }
                }
                Ok((_, Event::Connection)) => {}
                Err(RecvError::Lagged(_)) => transcripts.clear(),
                Err(RecvError::Closed) => return,
            },
            _ = ticker.tick() => {
                live.tick(&state);
                let now = now_ms();
                let due: Vec<(DeviceId, Notification)> = throttles
                    .iter_mut()
                    .flat_map(|(device, throttle)| {
                        throttle.due(now).into_iter().map(|n| (device.clone(), n)).collect::<Vec<_>>()
                    })
                    .collect();
                for (device, notification) in due {
                    dispatch(&state, &mut holds, &mut outbox, device, notification);
                }
                release(&state, &mut holds, &mut outbox, now);
                // Forget chats that left the layout.
                let layout = state.hub.layout();
                transcripts.retain(|pane, _| layout.chat(pane).is_some());
            }
        }
    }
}

/// The notifier's copy of one chat's transcript and the stream position
/// it includes.
struct Base {
    transcript: Transcript,
    cursor: Option<Cursor>,
}

impl Base {
    /// Whether the change that left the stream at `cursor` is in the copy.
    fn includes(&self, cursor: Cursor) -> bool {
        self.cursor
            .is_some_and(|base| base.epoch == cursor.epoch && cursor.seq <= base.seq)
    }
}

fn notification(
    state: &State,
    pane: &str,
    before: &Transcript,
    change: &herdr_chat_protocol::Change,
) -> Option<Notification> {
    let layout = state.hub.layout();
    let (workspace, row) = layout.chat(pane)?;
    let agent = if before.agent.is_empty() {
        row.agent.as_deref().unwrap_or("The agent")
    } else {
        &before.agent
    };
    decide(
        &Chat {
            agent,
            pane,
            title: &row.title,
            workspace: &workspace.label,
        },
        before,
        change,
        state.privacy,
    )
}

/// Offers `notification` to every subscribed device's throttle and sends
/// what passes now.
fn offer(
    state: &Arc<State>,
    throttles: &mut HashMap<DeviceId, Throttle>,
    holds: &mut HashMap<DeviceId, Hold>,
    outbox: &mut Outbox,
    notification: Notification,
) {
    let alerted = alert_devices(state);
    let subscribed: Vec<DeviceId> = state
        .auth()
        .devices()
        .list(|_| false)
        .into_iter()
        .filter(|device| device.push || alerted.contains(&device.id))
        .map(|device| device.id)
        .collect();
    throttles.retain(|device, _| subscribed.contains(device));
    outbox
        .queues
        .retain(|device, _| subscribed.contains(device));
    let now = now_ms();
    for device in subscribed {
        if let Some(passed) = throttles
            .entry(device.clone())
            .or_default()
            .offer(notification.clone(), now)
        {
            dispatch(state, holds, outbox, device, passed);
        }
    }
}

/// Sends `notification` to `device` now, or holds it while the device is
/// near.
fn dispatch(
    state: &Arc<State>,
    holds: &mut HashMap<DeviceId, Hold>,
    outbox: &mut Outbox,
    device: DeviceId,
    notification: Notification,
) {
    let passed = if state.near().is_near(&device, now_ms()) {
        holds.entry(device.clone()).or_default().hold(notification)
    } else {
        Some(notification)
    };
    if let Some(notification) = passed {
        outbox.send(state, device, notification);
    }
}

/// Sends what was held for devices no longer near, oldest first.
fn release(
    state: &Arc<State>,
    holds: &mut HashMap<DeviceId, Hold>,
    outbox: &mut Outbox,
    now_ms: u64,
) {
    let gone: Vec<DeviceId> = {
        let mut near = state.near();
        near.forget(now_ms);
        holds
            .keys()
            .filter(|device| !near.is_near(device, now_ms))
            .cloned()
            .collect()
    };
    for device in gone {
        let Some(mut hold) = holds.remove(&device) else {
            continue;
        };
        for notification in hold.release() {
            outbox.send(state, device.clone(), notification);
        }
    }
    holds.retain(|_, hold| !hold.is_empty());
}

/// Questions waiting across every chat, for the app badge.
pub(super) fn waiting(state: &State) -> u32 {
    let layout = state.hub.layout();
    let total: usize = layout
        .chats()
        .filter_map(|chat| state.hub.chat(&chat.pane))
        .map(|link| link.summary().questions)
        .sum();
    u32::try_from(total).unwrap_or(u32::MAX)
}

/// Notifications queued per device before new ones are dropped.
const OUTBOX: usize = 32;

/// One queue per device, sent in order by its own task: a question and the
/// clear that follows it share a topic, so the clear must not overtake it.
#[derive(Default)]
struct Outbox {
    queues: HashMap<DeviceId, mpsc::Sender<Notification>>,
}

impl Outbox {
    fn send(&mut self, state: &Arc<State>, device: DeviceId, notification: Notification) {
        let queue = self
            .queues
            .entry(device.clone())
            .or_insert_with(|| deliver(state.clone(), device.clone()));
        if let Err(mpsc::error::TrySendError::Closed(notification)) = queue.try_send(notification) {
            let queue = deliver(state.clone(), device.clone());
            let _ = queue.try_send(notification);
            self.queues.insert(device, queue);
        }
    }
}

/// Devices that get sealed alerts through APNs: those that registered an
/// alert token and a notify key, while the user's APNs key is set up.
fn alert_devices(state: &State) -> Vec<DeviceId> {
    if state.apns.is_none() {
        return Vec::new();
    }
    super::live::registrations(state)
        .into_iter()
        .filter(|(_, registration)| registration.alerts().is_some())
        .map(|(device, _)| device)
        .collect()
}

/// Starts the task that sends `device`'s notifications one at a time.
fn deliver(state: Arc<State>, device: DeviceId) -> mpsc::Sender<Notification> {
    let (queue, mut pending) = mpsc::channel(OUTBOX);
    tokio::spawn(async move {
        while let Some(notification) = pending.recv().await {
            if send_alert(&state, &device, &notification).await {
                continue;
            }
            let state = state.clone();
            let device = device.clone();
            let _ = tokio::task::spawn_blocking(move || send(&state, &device, &notification)).await;
        }
    });
    queue
}

/// Sends `notification` sealed through APNs when `device` registered for
/// alerts; `false` when it did not, so Web Push carries it instead.
async fn send_alert(state: &Arc<State>, device: &DeviceId, notification: &Notification) -> bool {
    let Some(sender) = &state.apns else {
        return false;
    };
    let registration = Registration::from_stored(
        state
            .auth()
            .devices()
            .get(device)
            .and_then(|record| record.apns.as_ref()),
    );
    let Some((token, key)) = registration.alerts() else {
        return false;
    };
    let (token, key) = (token.clone(), *key);
    let sealed = Sealed::new(
        notification,
        apns::computer_id(&state.fingerprint),
        waiting(state),
    );
    let Ok(push) = apns::alert::push(sealed, &notification.tag, |plaintext| {
        apns::seal::seal(&key, plaintext, apns::seal::INFO)
    }) else {
        return true;
    };
    if let Ok(apns::Outcome::Gone) = sender.post(&token, &push, now_ms() / 1000).await {
        let state = state.clone();
        let device = device.clone();
        let _ = tokio::task::spawn_blocking(move || forget(&state, &device, &token)).await;
    }
    true
}

/// Drops `token` from `device`'s registration after APNs refused it.
/// Blocking.
fn forget(state: &State, device: &DeviceId, token: &apns::DeviceToken) {
    let mut auth = state.auth();
    let devices = auth.devices_mut();
    let mut registration =
        Registration::from_stored(devices.get(device).and_then(|record| record.apns.as_ref()));
    if registration.forget(token)
        && let Ok(value) = registration.to_stored()
    {
        let _ = devices.set_apns(device, value);
    }
}

/// Sends one notification. Blocking.
fn send(state: &State, device: &DeviceId, notification: &Notification) {
    let Some(sender) = &state.push else {
        return;
    };
    let subscription = state
        .auth()
        .devices()
        .get(device)
        .and_then(|record| record.push.clone())
        .and_then(|value| serde_json::from_value::<Subscription>(value).ok());
    let Some(subscription) = subscription else {
        return;
    };
    let message = notification.message(waiting(state));
    let delivery = Delivery::for_notification(notification);
    if let Ok(Outcome::Gone) = sender.send(&subscription, &message, &delivery) {
        let _ = state.auth().devices_mut().set_push(device, None);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

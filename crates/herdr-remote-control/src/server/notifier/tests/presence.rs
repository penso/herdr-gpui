//! A near device's alerts wait in its hold and go out once it leaves.

use super::*;
use crate::push::{Kind, Urgency};

fn question(pane: &str) -> Notification {
    Notification {
        body: "Run tests?".to_owned(),
        clear: false,
        kind: Kind::Question,
        pane: pane.to_owned(),
        permission: None,
        tag: format!("chat-{pane}"),
        title: "Claude needs an answer".to_owned(),
        urgency: Urgency::High,
    }
}

#[tokio::test]
async fn a_near_devices_alerts_wait_until_it_leaves() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    let mut holds = HashMap::new();
    let mut outbox = Outbox::default();

    state.near().set(&device, true, now_ms());
    dispatch(
        &state,
        &mut holds,
        &mut outbox,
        device.clone(),
        question("w1:p2"),
    );
    assert!(outbox.queues.is_empty(), "held, not sent");
    assert!(holds.get(&device).is_some_and(|hold| !hold.is_empty()));

    // Still near: nothing goes.
    release(&state, &mut holds, &mut outbox, now_ms());
    assert!(outbox.queues.is_empty());

    state.near().set(&device, false, now_ms());
    release(&state, &mut holds, &mut outbox, now_ms());
    assert!(holds.is_empty());
    assert!(outbox.queues.contains_key(&device), "sent once away");
}

#[tokio::test]
async fn a_lapsed_near_mark_lets_held_alerts_go() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    let mut holds = HashMap::new();
    let mut outbox = Outbox::default();
    let now = now_ms();
    state.near().set(&device, true, now);
    dispatch(
        &state,
        &mut holds,
        &mut outbox,
        device.clone(),
        question("w1:p2"),
    );
    release(
        &state,
        &mut holds,
        &mut outbox,
        now + crate::push::NEAR_TTL_MS,
    );
    assert!(holds.is_empty());
    assert!(outbox.queues.contains_key(&device));
}

#[tokio::test]
async fn a_device_that_is_not_near_gets_alerts_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (device, _) = crate::web::tests::signed_in(&state, 3);
    let mut holds = HashMap::new();
    let mut outbox = Outbox::default();
    dispatch(
        &state,
        &mut holds,
        &mut outbox,
        device.clone(),
        question("w1:p2"),
    );
    assert!(holds.is_empty());
    assert!(outbox.queues.contains_key(&device));
}

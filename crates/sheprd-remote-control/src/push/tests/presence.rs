//! Holding a near device's alerts: one per chat, newest wins, a clear
//! drops what was held and passes, and near marks lapse.

use super::*;
use crate::control::DeviceId;
use crate::push::{Hold, MAX_HELD, NEAR_TTL_MS, Near};

fn notification(pane: &str, body: &str) -> Notification {
    Notification {
        body: body.to_owned(),
        clear: false,
        kind: Kind::Question,
        pane: pane.to_owned(),
        permission: None,
        tag: format!("chat-{pane}"),
        title: "t".to_owned(),
        urgency: Urgency::High,
    }
}

fn bodies(notifications: &[Notification]) -> Vec<&str> {
    notifications.iter().map(|n| n.body.as_str()).collect()
}

#[test]
fn a_near_mark_lasts_two_minutes_unless_renewed_or_lifted() {
    let device = DeviceId("d".into());
    let mut near = Near::default();
    assert!(!near.is_near(&device, 0));
    near.set(&device, true, 0);
    assert!(near.is_near(&device, NEAR_TTL_MS - 1));
    assert!(!near.is_near(&device, NEAR_TTL_MS));
    near.set(&device, true, NEAR_TTL_MS - 1);
    assert!(near.is_near(&device, NEAR_TTL_MS + 1_000));
    near.set(&device, false, NEAR_TTL_MS);
    assert!(!near.is_near(&device, NEAR_TTL_MS));
}

#[test]
fn held_alerts_keep_the_newest_per_chat_in_order() {
    let mut hold = Hold::default();
    assert!(hold.hold(notification("a", "a1")).is_none());
    assert!(hold.hold(notification("b", "b1")).is_none());
    assert!(hold.hold(notification("a", "a2")).is_none());
    assert_eq!(bodies(&hold.release()), ["b1", "a2"]);
    assert!(hold.is_empty());
}

#[test]
fn a_clear_drops_the_held_alert_and_goes_now() {
    let mut hold = Hold::default();
    hold.hold(notification("a", "a1"));
    hold.hold(notification("b", "b1"));
    let mut clear = notification("a", "");
    clear.clear = true;
    assert_eq!(hold.hold(clear).map(|n| n.clear), Some(true));
    assert_eq!(bodies(&hold.release()), ["b1"]);
}

#[test]
fn a_hold_is_bounded_and_drops_the_oldest() {
    let mut hold = Hold::default();
    for index in 0..=MAX_HELD {
        hold.hold(notification(&index.to_string(), &index.to_string()));
    }
    let released = hold.release();
    assert_eq!(released.len(), MAX_HELD);
    assert_eq!(released[0].body, "1");
}

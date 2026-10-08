#![allow(clippy::unwrap_used)]
//! How phone forwarding rides the shared notification policy.
use super::policy_tests::{config, endpoints, receive, visible, wire};
use super::*;
use crate::{config::NotificationConfig, endpoint::Endpoint};
use herdr_client::protocol::{AgentStatus, SemanticNotificationKind as Kind};
use std::sync::Arc;

fn phone(delivery: NotificationConfig) -> NotificationConfig {
    NotificationConfig {
        phone: phone::PhoneEvents {
            blocked: true,
            done: true,
        },
        ..delivery
    }
}

fn blocked(endpoints: &mut [Endpoint]) {
    Arc::make_mut(endpoints[1].live.snapshot.as_mut().unwrap()).agents[0].agent_status =
        AgentStatus::Blocked;
}

#[test]
fn phone_forwards_with_delivery_off_only_while_the_window_is_unfocused() {
    for focused in [false, true] {
        let now = Instant::now();
        let mut endpoints = endpoints();
        blocked(&mut endpoints);
        receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
        let mut custom = wire(Kind::Custom);
        custom.pane_id = None;
        receive(&mut endpoints[0], custom, now);
        let config = phone(NotificationConfig {
            delay_seconds: 0,
            ..Default::default()
        });
        tick(&mut endpoints, 0, config, false, focused, None, now);
        // Custom notices never reach the phone, so delivery Off drops them.
        assert!(endpoints[0].toasts.entries.is_empty());
        assert!(visible(&endpoints).is_empty());
        assert!(take_system(&mut endpoints, config).is_empty());
        let posts = take_phone(&mut endpoints, config.phone);
        if focused {
            assert!(posts.is_empty());
        } else {
            assert_eq!(
                posts,
                [PhonePost {
                    endpoint: 1,
                    tag: "herdr:remote:boot-v1:w1:p1".into(),
                    event: phone::Event::Blocked,
                    title: "event".into(),
                    body: Some("Review needed".into()),
                }]
            );
        }
        // Phone-only notices leave once taken and are never pushed twice.
        assert!(endpoints[1].toasts.entries.is_empty());
        tick(&mut endpoints, 0, config, false, focused, None, now);
        assert!(take_phone(&mut endpoints, config.phone).is_empty());
    }
}

#[test]
fn phone_ignores_active_tab_suppression_and_shares_shown_notices() {
    for (selected, toast) in [(1, false), (0, true)] {
        let now = Instant::now();
        let mut endpoints = endpoints();
        blocked(&mut endpoints);
        receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
        tick(
            &mut endpoints,
            selected,
            phone(config(0)),
            false,
            false,
            None,
            now,
        );
        assert_eq!(visible(&endpoints).len(), usize::from(toast));
        assert_eq!(take_phone(&mut endpoints, phone(config(0)).phone).len(), 1);
        // A shown toast stays after its push; a phone-only one does not.
        assert_eq!(endpoints[1].toasts.entries.len(), usize::from(toast));
        assert!(take_phone(&mut endpoints, phone(config(0)).phone).is_empty());
    }
}

#[test]
fn phone_waits_for_the_shared_delay_and_evidence() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[1], wire(Kind::Finished), now);
    let config = phone(config(1));
    tick(&mut endpoints, 0, config, false, false, None, now);
    assert!(take_phone(&mut endpoints, config.phone).is_empty());
    let later = now + Duration::from_secs(1);
    Arc::make_mut(endpoints[1].live.snapshot.as_mut().unwrap()).agents[0].agent_status =
        AgentStatus::Done;
    tick(&mut endpoints, 0, config, false, false, None, later);
    let posts = take_phone(&mut endpoints, config.phone);
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].event, phone::Event::Done);
}

#[test]
fn turning_phone_off_drops_pending_phone_only_notices_and_per_event_switches_apply() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    blocked(&mut endpoints);
    receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
    let on = phone(NotificationConfig {
        delay_seconds: 0,
        ..Default::default()
    });
    tick(&mut endpoints, 0, on, false, false, None, now);
    assert_eq!(endpoints[1].toasts.entries.len(), 1);
    tick(
        &mut endpoints,
        0,
        NotificationConfig::default(),
        false,
        false,
        None,
        now,
    );
    assert!(endpoints[1].toasts.entries.is_empty());
    // With only "done" forwarded, a blocked notice is not kept for the phone.
    receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
    let done_only = NotificationConfig {
        phone: phone::PhoneEvents {
            blocked: false,
            done: true,
        },
        ..Default::default()
    };
    tick(&mut endpoints, 0, done_only, false, false, None, now);
    assert!(endpoints[1].toasts.entries.is_empty());
    assert!(take_phone(&mut endpoints, done_only.phone).is_empty());
}

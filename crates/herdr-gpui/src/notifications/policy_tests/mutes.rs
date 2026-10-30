use super::*;
use crate::notification_mutes::{Daemon, Mutes, Scope};

fn tick_muted(endpoints: &mut [Endpoint], config: NotificationConfig, mutes: &Mutes, now: Instant) {
    super::super::tick(endpoints, 0, config, Some(mutes), false, true, None, now);
}

/// The fixture's workspace or tab muted on the second endpoint.
fn muted(endpoints: &[Endpoint], scope: Scope) -> Mutes {
    let mut mutes = Mutes::default();
    let id = match scope {
        Scope::Workspace => "w1",
        Scope::Tab => "w1:t1",
    };
    let snapshot = endpoints[1].live.snapshot.clone().unwrap();
    assert!(mutes.set(
        &Daemon::of(&endpoints[1]),
        &snapshot,
        scope,
        id,
        true,
        std::time::SystemTime::now()
    ));
    mutes
}

#[test]
fn a_muted_workspace_or_tab_shows_no_toast_and_posts_nothing() {
    for scope in [Scope::Workspace, Scope::Tab] {
        for base in [config(0), system(0)] {
            let now = Instant::now();
            let mut endpoints = endpoints();
            let mutes = muted(&endpoints, scope);
            receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
            tick_muted(&mut endpoints, base, &mutes, now);
            assert!(visible(&endpoints).is_empty(), "{scope:?}");
            assert!(take_system(&mut endpoints, base).is_empty(), "{scope:?}");
            assert!(endpoints[1].toasts.entries.is_empty(), "dropped, not held");
        }
    }
    // The same notices reach the user from a workspace nobody muted.
    for (base, in_app, os) in [(config(0), 1, 0), (system(0), 0, 1)] {
        let now = Instant::now();
        let mut endpoints = endpoints();
        receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
        tick_muted(&mut endpoints, base, &Mutes::default(), now);
        assert_eq!(visible(&endpoints).len(), in_app);
        assert_eq!(take_system(&mut endpoints, base).len(), os);
    }
}

#[test]
fn a_notice_naming_only_its_pane_follows_the_pane_into_a_muted_tab() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    let mutes = muted(&endpoints, Scope::Tab);
    let mut event = wire(Kind::NeedsAttention);
    event.workspace_id = None;
    event.tab_id = None;
    // Sent before the snapshot lists its parents to the notice.
    endpoints[1].toasts.receive([Notice::new(event, now)]);
    tick_muted(&mut endpoints, config(0), &mutes, now);
    assert!(endpoints[1].toasts.entries.is_empty());
}

#[test]
fn muting_takes_away_a_showing_toast_and_unmuting_brings_back_nothing() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
    let mut mutes = Mutes::default();
    tick_muted(&mut endpoints, config(0), &mutes, now);
    assert_eq!(visible(&endpoints), ["event"]);
    let daemon = Daemon::of(&endpoints[1]);
    let snapshot = endpoints[1].live.snapshot.clone().unwrap();
    mutes.set(
        &daemon,
        &snapshot,
        Scope::Workspace,
        "w1",
        true,
        std::time::SystemTime::now(),
    );
    tick_muted(&mut endpoints, config(0), &mutes, now);
    assert!(endpoints[1].toasts.entries.is_empty());
    mutes.set(
        &daemon,
        &snapshot,
        Scope::Workspace,
        "w1",
        false,
        std::time::SystemTime::now(),
    );
    tick_muted(&mut endpoints, config(0), &mutes, now);
    assert!(visible(&endpoints).is_empty());
    // What arrives after unmuting shows as before.
    receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
    tick_muted(&mut endpoints, config(0), &mutes, now);
    assert_eq!(visible(&endpoints), ["event"]);
}

#[test]
fn a_notice_waiting_out_its_delay_is_dropped_when_its_tab_is_muted() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
    let mut mutes = Mutes::default();
    tick_muted(&mut endpoints, config(5), &mutes, now);
    assert_eq!(endpoints[1].toasts.entries.len(), 1, "still waiting");
    let snapshot = endpoints[1].live.snapshot.clone().unwrap();
    mutes.set(
        &Daemon::of(&endpoints[1]),
        &snapshot,
        Scope::Tab,
        "w1:t1",
        true,
        std::time::SystemTime::now(),
    );
    tick_muted(
        &mut endpoints,
        config(5),
        &mutes,
        now + Duration::from_secs(6),
    );
    assert!(endpoints[1].toasts.entries.is_empty());
}

#[test]
fn a_mute_on_one_host_leaves_the_same_ids_on_another_alone() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    // Muted on "remote"; the notice comes from "local", whose snapshot
    // shows the same IDs.
    let mutes = muted(&endpoints, Scope::Workspace);
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    super::super::tick(
        &mut endpoints,
        1,
        config(0),
        Some(&mutes),
        false,
        true,
        None,
        now,
    );
    assert_eq!(visible(&endpoints), ["event"]);
}

#[test]
fn previews_and_local_feedback_show_whatever_is_muted() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    let mutes = muted(&endpoints, Scope::Workspace);
    let mut preview = wire(Kind::NeedsAttention);
    preview.title = "preview".into();
    endpoints[1]
        .toasts
        .receive([Notice::new(preview, now).preview()]);
    tick_muted(&mut endpoints, config(0), &mutes, now);
    assert_eq!(visible(&endpoints), ["preview"]);
}

#[test]
fn an_os_notification_posted_before_a_mute_still_answers_its_click() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[1], wire(Kind::NeedsAttention), now);
    let mut mutes = Mutes::default();
    tick_muted(&mut endpoints, system(0), &mutes, now);
    assert_eq!(take_system(&mut endpoints, system(0)).len(), 1);
    let snapshot = endpoints[1].live.snapshot.clone().unwrap();
    mutes.set(
        &Daemon::of(&endpoints[1]),
        &snapshot,
        Scope::Workspace,
        "w1",
        true,
        std::time::SystemTime::now(),
    );
    tick_muted(&mut endpoints, system(0), &mutes, now);
    assert_eq!(endpoints[1].toasts.entries.len(), 1);
    assert!(endpoints[1].toasts.entries[0].1.posted);
}

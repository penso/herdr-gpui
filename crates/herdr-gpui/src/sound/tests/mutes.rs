use super::*;

/// A service whose worker is a channel, with the default one second delay.
fn service() -> (Service, mpsc::Receiver<Job>) {
    let (sender, receiver) = mpsc::sync_channel(8);
    let service = Service {
        sender: Some(sender),
        settings: Arc::new(Mutex::new(Some(Arc::new(Settings::default())))),
        reload: Default::default(),
        stop: Default::default(),
        volume: Default::default(),
    };
    (service, receiver)
}

#[test]
fn a_muted_place_plays_nothing_and_unmuting_does_not_replay_it() {
    let (service, played) = service();
    let mut state = live(AgentStatus::Idle);
    let mut policy = Policy::default();
    let now = Instant::now();
    state.sound_events.push_back((now, event(Kind::Custom)));
    service.poll(&mut policy, &mut state, false, now, |_| true);
    assert!(played.try_recv().is_err());
    assert!(state.sound_events.is_empty());
    service.poll(&mut policy, &mut state, false, now, |_| false);
    assert!(played.try_recv().is_err(), "nothing replays after unmuting");
    state.sound_events.push_back((now, event(Kind::Custom)));
    service.poll(&mut policy, &mut state, false, now, |_| false);
    assert!(played.try_recv().is_ok());
}

#[test]
fn muting_while_a_sound_waits_out_its_delay_drops_it() {
    let (service, played) = service();
    let mut state = live(AgentStatus::Blocked);
    let mut policy = Policy::default();
    let now = Instant::now();
    state
        .sound_events
        .push_back((now, event(Kind::NeedsAttention)));
    service.poll(&mut policy, &mut state, false, now, |_| false);
    assert_eq!(policy.pending.len(), 1, "waiting for the delay");
    let later = now + Duration::from_secs(2);
    service.poll(&mut policy, &mut state, false, later, |_| true);
    assert!(policy.pending.is_empty());
    service.poll(&mut policy, &mut state, false, later, |_| false);
    assert!(played.try_recv().is_err());
}

#[test]
fn only_the_muted_place_is_silent() {
    let (service, played) = service();
    let mut state = live(AgentStatus::Idle);
    let mut policy = Policy::default();
    let now = Instant::now();
    let mut other = event(Kind::Custom);
    other.workspace_id = Some("other".into());
    other.tab_id = None;
    other.pane_id = None;
    state.sound_events.push_back((now, event(Kind::Custom)));
    state.sound_events.push_back((now, other));
    service.poll(&mut policy, &mut state, false, now, |event| {
        event.workspace_id.as_deref() == Some("workspace")
    });
    let job = played.try_recv().unwrap();
    assert!(matches!(
        job.request,
        PlaybackRequest::Notification { event, .. } if event.workspace_id.as_deref() == Some("other")
    ));
    assert!(played.try_recv().is_err());
}

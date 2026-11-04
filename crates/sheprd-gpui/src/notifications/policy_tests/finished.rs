use super::*;

/// The agent in the fixture's pane is in the state its notice claims, so
/// only the `finished` switch decides whether the notice is presented.
fn deliver(kind: Kind, base: NotificationConfig, finished: bool) -> (Vec<String>, usize) {
    let now = Instant::now();
    let mut endpoints = endpoints();
    Arc::make_mut(endpoints[1].live.snapshot.as_mut().unwrap()).agents[0].agent_status = match kind
    {
        Kind::NeedsAttention => AgentStatus::Blocked,
        _ => AgentStatus::Done,
    };
    receive(&mut endpoints[1], wire(kind), now);
    let config = NotificationConfig { finished, ..base };
    tick(&mut endpoints, 0, config, false, now);
    let posts = take_system(&mut endpoints, config).len();
    let shown = visible(&endpoints).into_iter().map(str::to_owned).collect();
    (shown, posts)
}

#[test]
fn muting_finished_keeps_attention_and_custom_in_app_and_in_the_os_center() {
    for (base, system) in [(config(0), false), (system(0), true)] {
        for kind in [Kind::Finished, Kind::NeedsAttention, Kind::Custom] {
            for finished in [true, false] {
                let (shown, posts) = deliver(kind, base, finished);
                let expected = usize::from(finished || kind != Kind::Finished);
                let (in_app, os) = if system { (0, expected) } else { (expected, 0) };
                assert_eq!(shown.len(), in_app, "{kind:?} finished={finished}");
                assert_eq!(posts, os, "{kind:?} finished={finished}");
            }
        }
    }
}

#[test]
fn muting_finished_drops_waiting_notices_but_not_previews() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[1], wire(Kind::Finished), now);
    let mut preview = wire(Kind::Finished);
    preview.title = "preview".into();
    endpoints[0]
        .toasts
        .receive([Notice::new(preview, now).preview()]);
    // Still waiting out the delay when a reload turns finished notices off.
    tick(&mut endpoints, 0, config(5), false, now);
    assert_eq!(endpoints[1].toasts.entries.len(), 1);
    let muted = NotificationConfig {
        finished: false,
        ..config(5)
    };
    tick(&mut endpoints, 0, muted, false, now);
    assert!(endpoints[1].toasts.entries.is_empty());
    assert_eq!(visible(&endpoints), ["preview"]);
}

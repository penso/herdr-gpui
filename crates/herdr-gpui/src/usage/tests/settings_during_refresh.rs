use super::super::{Host, Message, Usage, Worker};
use super::*;
use std::{collections::HashSet, sync::mpsc};

#[test]
fn changing_providers_during_a_read_preserves_cache_and_queues_latest_preferences() {
    let now = Instant::now();
    let codex = provider("codex");
    let claude = provider("claude");
    let old = UsageConfig {
        hide_providers: vec!["codex".into()],
        ..Default::default()
    };
    let latest = UsageConfig::default();
    let granted = HashSet::new();
    let mut usage = Usage::default();
    usage.poll(Some(Host::Local), &latest, &granted, 0, false, now);
    begin(&mut usage, &Host::Local, now);
    usage.apply(
        Message::Reading(Host::Local, codex, Ok(report(codex, 20.))),
        &latest,
        now,
    );
    usage.apply(Message::Done(Host::Local, Ok(())), &latest, now);
    usage.poll(Some(Host::Local), &old, &granted, 1, false, now);
    begin(&mut usage, &Host::Local, now);
    usage.apply(
        Message::Reading(Host::Local, claude, Ok(report(claude, 5.))),
        &old,
        now,
    );

    usage.poll(Some(Host::Local), &latest, &granted, 2, false, now);
    usage.apply(Message::Done(Host::Local, Ok(())), &latest, now);
    assert!(
        usage
            .current()
            .unwrap()
            .headline(&latest, 2, None)
            .iter()
            .any(|reading| reading.provider == codex)
    );

    let (requests, _incoming) = mpsc::sync_channel(1);
    let (outgoing, results) = mpsc::sync_channel(2);
    usage.worker = Some(Worker { requests, results });
    usage.poll(Some(Host::Local), &latest, &granted, 2, true, now);
    assert!(
        usage.busy(),
        "the old round must not defer the settings change for ten minutes"
    );
    outgoing
        .send(Message::Reading(Host::Local, codex, Ok(report(codex, 85.))))
        .unwrap();
    outgoing.send(Message::Done(Host::Local, Ok(()))).unwrap();
    usage.poll(Some(Host::Local), &latest, &granted, 2, false, now);
    let shown = usage.current().unwrap().headline(&latest, 2, None);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].provider, codex);
    assert_eq!(shown[0].report.as_ref().unwrap().windows[0].used, 85.);
    assert!(!usage.busy());
}

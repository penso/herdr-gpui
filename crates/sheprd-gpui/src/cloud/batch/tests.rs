#![allow(clippy::unwrap_used)]
use super::*;
use std::time::{Duration, Instant};

fn target(id: &str) -> Target {
    let provider = CloudProvider::ALL[0];
    Target {
        provider,
        id: id.into(),
        endpoint: format!("{}:{id}", provider.key()),
        label: id.to_uppercase(),
    }
}

fn states(batch: &BatchInfo) -> Vec<DeviceState> {
    batch.devices.iter().map(|device| device.state).collect()
}

/// Polls until the batch settles; worker threads report on their own.
fn settled(registry: &Registry, id: u64) -> BatchInfo {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let batch = registry.get(id).unwrap();
        if batch.phase.settled() {
            return batch;
        }
        assert!(Instant::now() < deadline, "{batch:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn closing_the_question_unanswered_cancels_every_device() {
    let registry = Registry::default();
    let pending = registry.open_destroy(&[target("a"), target("b")]);
    let id = pending.id();
    assert_eq!(registry.get(id).unwrap().phase, Phase::Confirming);
    drop(pending);
    let batch = registry.get(id).unwrap();
    assert_eq!(batch.phase, Phase::Cancelled);
    assert_eq!(states(&batch), [DeviceState::Cancelled; 2]);
}

#[test]
fn a_confirmed_batch_is_done_once_every_device_settles() {
    let registry = Registry::default();
    let pending = registry.open_destroy(&[target("a"), target("b"), target("c")]);
    let (registry, id, targets) = pending.confirm().unwrap();
    assert_eq!(targets.len(), 3);
    destroy::start(&registry, id, targets, |target| match target.id.as_str() {
        "a" => Ok(DeviceState::Destroyed),
        "b" => Ok(DeviceState::Forgotten),
        _ => Err(crate::cloud::Error::Missing.into()),
    });
    let batch = settled(&registry, id);
    assert_eq!(batch.phase, Phase::Done);
    assert_eq!(
        states(&batch),
        [
            DeviceState::Destroyed,
            DeviceState::Forgotten,
            DeviceState::Failed
        ]
    );
    assert_eq!(
        batch.devices[2].error.as_deref(),
        Some("The cloud device is no longer saved.")
    );
    assert!(!batch.succeeded());
}

#[test]
fn a_batch_answered_once_cannot_be_answered_again() {
    let registry = Registry::default();
    let pending = registry.open_destroy(&[target("a")]);
    let id = pending.id();
    registry.cancel(id);
    assert!(
        pending.confirm().is_none(),
        "a cancelled batch never starts"
    );
    assert_eq!(registry.get(id).unwrap().phase, Phase::Cancelled);
    let pending = registry.open_destroy(&[target("a")]);
    let id = pending.id();
    let _ = pending.confirm().unwrap();
    registry.cancel(id);
    assert_eq!(registry.get(id).unwrap().phase, Phase::Running);
}

#[test]
fn only_the_newest_batches_are_kept_and_failures_are_bounded() {
    let registry = Registry::default();
    let first = registry.open_destroy(&[target("a")]).id();
    let mut last = first;
    for _ in 0..KEPT {
        last = registry.open_destroy(&[target("a")]).id();
    }
    assert!(registry.get(first).is_none());
    assert_eq!(registry.all().len(), KEPT);
    assert_eq!(registry.all().last().unwrap().id, last);
    registry.update(
        last,
        0,
        DeviceState::Failed,
        Some(&"x".repeat(ERROR_LIMIT * 2)),
    );
    let error = registry.get(last).unwrap().devices[0]
        .error
        .clone()
        .unwrap();
    assert_eq!(error.chars().count(), ERROR_LIMIT);
    // An index past the batch, or a batch gone, changes nothing.
    registry.update(last, 9, DeviceState::Destroyed, None);
    registry.update(first, 0, DeviceState::Destroyed, None);
}

#[test]
fn a_saved_machine_counts_until_the_window_lists_it() {
    let registry = Registry::default();
    let mut waiting = target("a").info();
    waiting.endpoint.clear();
    let pending = registry.open(BatchKind::Create, vec![waiting], ());
    let (registry, id, ()) = pending.confirm().unwrap();
    assert_eq!(registry.creating(|_| false), 1);
    let made = target("a");
    registry.created(id, 0, &made);
    registry.update(id, 0, DeviceState::Ready, None);
    assert_eq!(registry.creating(|_| false), 1, "saved but not yet listed");
    assert_eq!(registry.creating(|endpoint| endpoint == made.endpoint), 0);
}

#[test]
fn a_running_batch_outlives_the_history_bound() {
    let registry = Registry::default();
    let mut waiting = target("a").info();
    waiting.endpoint.clear();
    let (registry, running, ()) = registry
        .open(BatchKind::Create, vec![waiting], ())
        .confirm()
        .unwrap();
    for _ in 0..KEPT * 2 {
        drop(registry.open_destroy(&[target("b")]));
    }
    assert!(registry.get(running).is_some());
    assert_eq!(registry.all().len(), KEPT);
    assert_eq!(registry.creating(|_| false), 1);
}

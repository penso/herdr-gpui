//! A source syncing again keeps the count its last sync reported, so the
//! view need not drop it, even when it was nothing.
use super::*;

#[test]
fn a_sync_keeps_the_last_count_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, _host) = opened(dir.path());
    let key = worker.snapshot.sources[0].key.clone();
    let state = |worker: &Worker| worker.snapshot.sources[0].state.clone();

    worker.set_state(&key, SyncState::Syncing { count: None });
    worker.set_syncing(&key);
    assert_eq!(state(&worker).count(), None, "a first sync has no count");

    for count in [0, 12] {
        worker.set_state(
            &key,
            SyncState::Synced {
                at: Utc::now(),
                count,
            },
        );
        worker.set_syncing(&key);
        assert!(
            matches!(state(&worker), SyncState::Syncing { count: Some(kept) } if kept == count)
        );
    }

    worker.set_state(&key, SyncState::Capped { count: 3 });
    worker.set_syncing(&key);
    assert_eq!(state(&worker).count(), Some(3));

    // A failure reported no count, so the next sync has none to keep.
    worker.set_state(&key, SyncState::Failed(Arc::new(Error::Cancelled)));
    worker.set_syncing(&key);
    assert_eq!(state(&worker).count(), None);
}

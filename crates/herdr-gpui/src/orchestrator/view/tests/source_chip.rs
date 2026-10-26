//! A backend's chip keeps its count while it syncs, with a spinner for a dot.
use crate::orchestrator::{
    SyncState,
    view::inbox::{SourceState, source_summary},
};

#[test]
fn a_sync_keeps_the_count_the_chip_showed() {
    let synced = SyncState::Synced {
        at: chrono::Utc::now(),
        count: 5,
    };
    // One repository syncing again, another done: both counts stay.
    assert_eq!(
        source_summary(&[(&SyncState::Syncing { count: None }, 3), (&synced, 5)]),
        (SourceState::Syncing, "8".to_owned())
    );
    // Syncing alone counts the items it already shows.
    assert_eq!(
        source_summary(&[(&SyncState::Syncing { count: None }, 335)]),
        (SourceState::Syncing, "335".to_owned())
    );
    // A later sync keeps the count its last one reported, even nothing.
    assert_eq!(
        source_summary(&[(&SyncState::Syncing { count: Some(0) }, 0)]),
        (SourceState::Syncing, "0".to_owned())
    );
    assert_eq!(
        source_summary(&[(&SyncState::Syncing { count: Some(7) }, 4)]),
        (SourceState::Syncing, "7".to_owned())
    );
    // A first sync has nothing to count yet.
    assert_eq!(
        source_summary(&[(&SyncState::Syncing { count: None }, 0)]),
        (SourceState::Syncing, String::new())
    );
    assert_eq!(
        source_summary(&[(&synced, 5)]),
        (SourceState::Synced, "5".to_owned())
    );
}

#[test]
fn a_failure_or_sign_in_shows_once_nothing_syncs() {
    assert_eq!(
        source_summary(&[(&SyncState::SignIn, 0)]).0,
        SourceState::SignIn
    );
    assert_eq!(
        source_summary(&[
            (&SyncState::SignIn, 0),
            (&SyncState::Syncing { count: None }, 2)
        ]),
        (SourceState::Syncing, "2".to_owned())
    );
    assert_eq!(
        source_summary(&[(&SyncState::Capped { count: 3 }, 3)]),
        (SourceState::Synced, "3".to_owned())
    );
    // Nothing listed is still a count once a sync reported it.
    let empty = SyncState::Synced {
        at: chrono::Utc::now(),
        count: 0,
    };
    assert_eq!(
        source_summary(&[(&empty, 0)]),
        (SourceState::Synced, "0".to_owned())
    );
    assert_eq!(
        source_summary(&[(&SyncState::Unsupported, 0)]),
        (SourceState::Unsupported, "not supported yet".to_owned())
    );
}

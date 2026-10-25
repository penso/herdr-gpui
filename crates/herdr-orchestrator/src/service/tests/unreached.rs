//! A host that cannot be reached answered nothing about the checkout, so
//! the repository last shown stays, beside the failure, until a sync finds
//! it again; a host that answers otherwise still drops it.
use super::*;
use crate::cache::path_in;
use std::process::ExitStatus;

fn exited(code: u8) -> ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(i32::from(code) << 8)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(u32::from(code))
    }
}

/// Detection whose `ssh` could not reach the host.
fn unreached() -> Error {
    Error::Script {
        operation: "Reading the repository",
        source: herdr_client::Error::ScriptExit {
            status: exited(255),
            stderr: "kex_exchange_identification: read: Connection reset by peer".into(),
        },
    }
}

fn cached(worker: &Worker) -> Option<RepoInfo> {
    let cache = Cache::open_at(&path_in(worker.request.data_root.as_ref().unwrap())).unwrap();
    let key = cache_key(&worker.request.target, &worker.request.checkout);
    cache
        .get::<RepoInfo>(Kind::Repository, &key)
        .unwrap()
        .map(|entry| entry.value)
}

#[test]
fn an_unreached_host_keeps_the_repository_shown_until_a_sync_finds_it() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host) = opened(dir.path());
    let shown = worker.snapshot.repo.clone();
    worker.found(&host, Err(unreached())).unwrap();
    assert_eq!(worker.snapshot.repo, shown, "the listing stays");
    assert!(worker.store.is_some());
    assert!(
        worker
            .snapshot
            .error
            .as_deref()
            .is_some_and(|error| matches!(
                error,
                Error::Script { source, .. } if source.host_unreached()
            ))
    );
    assert_eq!(cached(&worker), shown, "the next start shows it too");
    // The next sync finds the repository first, and the failure goes.
    worker.sync(None);
    assert!(!worker.unreached);
    assert!(worker.snapshot.error.is_none());
    assert_eq!(worker.snapshot.repo, shown);
}

#[test]
fn an_unreached_host_with_nothing_shown_is_found_on_a_later_sync() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host) = opened(dir.path());
    let found = worker.snapshot.repo.clone();
    worker.forget_repository();
    worker.found(&host, Err(unreached())).unwrap();
    assert!(worker.snapshot.repo.is_none());
    assert!(worker.snapshot.error.is_some());
    worker.sync(None);
    assert!(worker.snapshot.error.is_none());
    assert_eq!(worker.snapshot.repo, found);
}

#[test]
fn a_host_that_answers_otherwise_still_drops_the_repository() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host) = opened(dir.path());
    worker.found(&host, Err(unreached())).unwrap();
    let error = worker.found(&host, Err(Error::NotARepository)).unwrap_err();
    assert!(matches!(error, Error::NotARepository));
    assert!(!worker.unreached, "a later sync does not look again");
    assert!(worker.snapshot.repo.is_none());
    assert!(worker.store.is_none());
    assert_eq!(cached(&worker), None);
}

#[test]
fn a_cached_repository_whose_rows_cannot_be_read_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, worker, _) = opened(dir.path());
    let database = worker.site.as_ref().unwrap().database.clone();
    let request = worker.request.clone();
    drop(worker);
    // A saved row this version cannot read: showing the cached repository
    // fails after it was set, so detection must show it again and fail too.
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute(
            "INSERT INTO issues (canonical_key, source, provider, host, repository, native_id, \
             identifier, title, state, labels_json, blocked_by_json) \
             VALUES ('k', 's', 'nonsense', 'h', 'r', '1', '#1', 't', 'open', '[]', '[]')",
            [],
        )
        .unwrap();
    let (_service, mut worker, _) = Service::prepare(request);
    assert!(worker.open().is_err());
}

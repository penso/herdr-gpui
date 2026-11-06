#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn releasing_the_journal_unlocks_even_while_a_duplicate_descriptor_lives() {
    let dir = tempfile::tempdir().unwrap();
    let settings = crate::boat::tests::settings();
    let journal = Journal::open(dir.path(), &settings).unwrap();
    let duplicate = journal._lock.try_clone().unwrap();
    drop(journal);
    let next = Journal::open(dir.path(), &settings).unwrap();
    drop(duplicate);
    assert!(matches!(
        Journal::open(dir.path(), &settings),
        Err(Error::PendingBusy)
    ));
    drop(next);
    assert!(Journal::open(dir.path(), &settings).is_ok());
}

#[test]
fn reopening_reuses_the_exact_request_even_after_settings_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = crate::boat::tests::settings();
    let journal = Journal::open(dir.path(), &settings).unwrap();
    let original = journal.begin_at(&settings, "original", 100).unwrap();
    assert!(matches!(
        Journal::open(dir.path(), &settings),
        Err(Error::PendingBusy)
    ));
    drop(journal);
    settings.machine_type = Some("large".into());
    settings.environment = Some("other".into());
    settings.ttl_seconds = Some(3600);
    let journal = Journal::open(dir.path(), &settings).unwrap();
    assert_eq!(
        journal.begin_at(&settings, "replacement", 101).unwrap(),
        original
    );
    journal.clear().unwrap();
    let next = journal.begin_at(&settings, "replacement", 102).unwrap();
    assert_ne!(next.key, original.key);
    assert_eq!(next.payload.ttl_seconds, Some(3600));
}

#[test]
fn expired_or_future_requests_are_retained_and_never_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let settings = crate::boat::tests::settings();
    let journal = Journal::open(dir.path(), &settings).unwrap();
    let original = journal.begin_at(&settings, "original", 100).unwrap();
    for now in [99, 100 + RETRY_WINDOW, 100 + 24 * 3600] {
        assert!(matches!(
            journal.begin_at(&settings, "new", now),
            Err(Error::PendingExpired)
        ));
        assert_eq!(
            read(&journal.path, &settings).unwrap(),
            Some(original.clone())
        );
    }
}

#[test]
fn accounts_are_isolated_and_corrupt_records_block_creation() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = crate::boat::tests::settings();
    let first = Journal::open(dir.path(), &settings).unwrap();
    first.begin_at(&settings, "one", 100).unwrap();
    settings.organization = Some("other".into());
    let second = Journal::open(dir.path(), &settings).unwrap();
    let original = second.begin_at(&settings, "two", 100).unwrap();
    assert_ne!(first.path, second.path);
    fs::write(&second.path, "broken").unwrap();
    assert!(matches!(
        second.begin_at(&settings, "three", 101),
        Err(Error::Json(_))
    ));
    fs::write(&second.path, vec![b' '; LIMIT as usize + 1]).unwrap();
    assert!(matches!(
        second.begin_at(&settings, "three", 101),
        Err(Error::Field(_))
    ));
    assert!(!original.key.is_empty());
}

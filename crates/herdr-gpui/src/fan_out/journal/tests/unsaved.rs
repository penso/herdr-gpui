use super::*;

#[test]
fn a_fan_out_beyond_the_journal_keeps_its_steps_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let journal = Journal::at(Some(path.clone()));
    for id in 0..MAX_RECORDS as u64 {
        journal.begin(record(id)).unwrap().unwrap().write().unwrap();
    }
    assert!(matches!(journal.begin(record(99)), Err(Error::JournalFull)));
    journal
        .record(99, &lane(0, Progress::CreatingWorktree))
        .unwrap();
    journal
        .record(99, &lane(0, Progress::Created(checkout(1))))
        .unwrap();
    journal
        .record(99, &lane(0, Progress::StartingAgent))
        .unwrap();
    // A failed agent start is retried from where it stopped, never twice.
    assert_eq!(journal.step(99, 0), Some(Checkpoint::StartingAgent));
    assert_eq!(journal.step(99, 1), Some(Checkpoint::Planned));
    // Nothing of it reaches the disk or another window.
    let saved = Journal::at(Some(path)).records();
    assert!(saved.iter().all(|record| record.id != 99));
    assert!(journal.records().iter().all(|record| record.id != 99));
    assert!(journal.claim(|record| record.id == 99).is_none());
    assert!(journal.forget(99).is_none());
    assert_eq!(journal.step(99, 0), None);
}

#[test]
fn a_fan_out_with_a_field_too_long_is_followed_in_memory() {
    let journal = Journal::default();
    let mut origin = origin();
    origin.repo_label = "x".repeat(MAX_FIELD_BYTES + 1);
    let long = Record::new(5, &origin, "fix the bug", &lanes());
    assert!(matches!(
        journal.begin(long),
        Err(Error::JournalFieldTooLong)
    ));
    journal
        .record(5, &lane(1, Progress::CreatingWorktree))
        .unwrap();
    assert_eq!(journal.step(5, 1), Some(Checkpoint::CreatingWorktree));
    // Settling a lane that has nothing left lets the fan-out go.
    let _ = journal.settle(5, &[]);
    assert_eq!(journal.step(5, 1), Some(Checkpoint::CreatingWorktree));
    let _ = journal.settle(5, &[1]);
    assert_eq!(journal.step(5, 0), None);
}

#[test]
fn fan_outs_followed_in_memory_are_bounded() {
    let journal = Journal::default();
    for id in 0..MAX_RECORDS as u64 {
        assert!(journal.begin(record(id)).is_ok());
    }
    let first = 100;
    for id in first..first + MAX_UNSAVED as u64 + 1 {
        assert!(journal.begin(record(id)).is_err());
    }
    assert_eq!(journal.step(first, 0), None, "the oldest is let go");
    assert_eq!(journal.step(first + 1, 0), Some(Checkpoint::Planned));
}

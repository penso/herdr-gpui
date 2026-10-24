use super::*;
use crate::fan_out::plan::Picks;

pub(crate) fn origin() -> Origin {
    Origin {
        endpoint_id: "local".into(),
        endpoint_label: "This Mac".into(),
        host: crate::teleport::host_for(&herdr_client::ConnectTarget::Local).unwrap(),
        workspace_id: "w0".into(),
        repo_key: "/tmp/repo/.git".into(),
        repo_label: "repo".into(),
        base: "HEAD".into(),
    }
}

pub(crate) fn lanes() -> Vec<Lane> {
    let mut picks = Picks::default();
    picks.add(AgentKind::Claude);
    picks.add(AgentKind::Codex);
    crate::fan_out::plan::lanes(&picks, 7)
}

pub(crate) fn checkout(n: usize) -> Checkout {
    Checkout {
        endpoint_id: "local".into(),
        host: origin().host,
        workspace_id: format!("w{n}"),
        path: format!("/tmp/lane-{n}"),
        pane_id: format!("p{n}"),
    }
}

fn record(id: u64) -> Record {
    Record::new(id, &origin(), "fix the bug", &lanes())
}

fn lane(index: usize, progress: Progress) -> Report {
    Report::Lane(index, progress)
}

#[test]
fn each_step_is_recorded_before_and_after_it_runs() {
    let mut record = record(1);
    assert_eq!(record.lanes[0].step, Checkpoint::Planned);
    let steps = [
        (Progress::CreatingWorktree, Checkpoint::CreatingWorktree),
        (Progress::Created(checkout(1)), Checkpoint::Created),
        (Progress::StartingAgent, Checkpoint::StartingAgent),
        (Progress::Prompting, Checkpoint::Prompting),
        (Progress::Running, Checkpoint::Prompted),
    ];
    for (progress, step) in steps {
        assert!(record.advance(&lane(0, progress)));
        assert_eq!(record.lanes[0].step, step);
    }
    assert_eq!(
        record.lanes[0].checkout,
        Some(SavedCheckout::from(&checkout(1)))
    );
    // The other lane never moved.
    assert_eq!(record.lanes[1].step, Checkpoint::Planned);
    assert!(record.valid());
}

#[test]
fn failures_and_host_setup_leave_a_lane_where_it_stopped() {
    let mut record = record(1);
    record.advance(&lane(0, Progress::CreatingWorktree));
    assert!(!record.advance(&lane(0, Progress::SettingUp)));
    assert!(!record.advance(&lane(0, Progress::Failed(Error::Cancelled))));
    assert_eq!(record.lanes[0].step, Checkpoint::CreatingWorktree);
    assert!(!record.advance(&lane(9, Progress::CreatingWorktree)));
}

#[test]
fn a_lane_never_claims_an_agent_without_a_worktree_nor_forgets_its_worktree() {
    let mut record = record(1);
    assert!(!record.advance(&lane(0, Progress::StartingAgent)));
    assert_eq!(record.lanes[0].step, Checkpoint::Planned);
    record.advance(&lane(0, Progress::Created(checkout(1))));
    assert!(!record.advance(&lane(0, Progress::CreatingWorktree)));
    assert_eq!(record.lanes[0].step, Checkpoint::Created);
    assert!(record.valid());
}

#[test]
fn the_base_commit_is_recorded_once_resolved() {
    let mut record = record(1);
    assert!(record.advance(&Report::Base("abc".into())));
    assert!(!record.advance(&Report::Base("abc".into())));
    assert_eq!(record.commit.as_deref(), Some("abc"));
}

#[test]
fn a_prompt_too_long_to_keep_is_dropped() {
    let long = "x".repeat(MAX_PROMPT_BYTES + 1);
    assert_eq!(Record::new(1, &origin(), &long, &lanes()).prompt, None);
    let fits = "x".repeat(MAX_PROMPT_BYTES);
    assert_eq!(
        Record::new(1, &origin(), &fits, &lanes()).prompt.as_deref(),
        Some(fits.as_str())
    );
}

#[test]
fn only_lanes_that_may_have_a_worktree_are_worth_keeping() {
    let mut record = record(1);
    assert!(!record.worth_keeping());
    record.advance(&lane(1, Progress::CreatingWorktree));
    assert!(record.worth_keeping());
}

#[test]
fn only_created_or_half_prompted_lanes_resume_and_never_by_starting_twice() {
    assert_eq!(Checkpoint::Planned.resume(), None);
    assert_eq!(Checkpoint::CreatingWorktree.resume(), None);
    assert_eq!(Checkpoint::Created.resume(), Some(Resume::Start));
    assert_eq!(Checkpoint::StartingAgent.resume(), Some(Resume::Prompt));
    assert_eq!(Checkpoint::Prompting.resume(), Some(Resume::Prompt));
    assert_eq!(Checkpoint::Prompted.resume(), None);
}

#[test]
fn records_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let journal = Journal::at(Some(path.clone()));
    journal.begin(record(1)).unwrap().unwrap().write().unwrap();
    journal
        .advance(1, &lane(0, Progress::Created(checkout(1))))
        .unwrap()
        .write()
        .unwrap();
    let restarted = Journal::at(Some(path));
    let records = restarted.records();
    assert_eq!(records, journal.records());
    assert_eq!(records[0].lanes[0].step, Checkpoint::Created);
}

#[test]
fn an_older_change_never_overwrites_a_newer_one_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let journal = Journal::at(Some(path.clone()));
    let older = journal.begin(record(1)).unwrap().unwrap();
    let newer = journal
        .advance(1, &lane(0, Progress::CreatingWorktree))
        .unwrap();
    newer.write().unwrap();
    older.write().unwrap();
    let records = Journal::at(Some(path)).records();
    assert_eq!(records[0].lanes[0].step, Checkpoint::CreatingWorktree);
}

#[test]
fn an_unsaved_journal_has_nothing_to_write() {
    let journal = Journal::default();
    assert!(journal.begin(record(1)).unwrap().is_none());
    assert_eq!(journal.records().len(), 1);
    assert!(journal.forget(1).is_none());
    assert!(journal.records().is_empty());
}

#[test]
fn a_full_journal_drops_no_saved_fan_out_for_a_new_one() {
    let journal = Journal::default();
    for id in 0..MAX_RECORDS as u64 {
        assert!(journal.begin(record(id)).is_ok());
    }
    assert!(matches!(journal.begin(record(99)), Err(Error::JournalFull)));
    let ids: Vec<u64> = journal.records().iter().map(|r| r.id).collect();
    assert_eq!(ids, (0..MAX_RECORDS as u64).collect::<Vec<_>>());
    // Relaunching a journaled fan-out replaces it rather than counting twice.
    assert!(journal.begin(record(0)).is_ok());
}

#[test]
fn settled_lanes_are_dropped_and_the_fan_out_once_nothing_may_be_left() {
    let journal = Journal::default();
    let _ = journal.begin(record(1));
    let _ = journal.advance(1, &lane(0, Progress::CreatingWorktree));
    let _ = journal.advance(1, &lane(1, Progress::CreatingWorktree));
    let _ = journal.settle(1, &[0]);
    let records = journal.records();
    assert_eq!(records[0].lanes.len(), 1);
    assert_eq!(records[0].lanes[0].kind, "codex");
    let _ = journal.settle(1, &[0]);
    assert!(journal.records().is_empty());
    let _ = journal.begin(record(2));
    let _ = journal.forget(2);
    assert!(journal.records().is_empty());
}

#[test]
fn settling_keeps_a_fan_out_whose_creation_may_have_happened() {
    let journal = Journal::default();
    let _ = journal.begin(record(1));
    let _ = journal.settle(1, &[]);
    assert!(journal.records().is_empty());
    let _ = journal.begin(record(2));
    let _ = journal.advance(2, &lane(1, Progress::CreatingWorktree));
    let _ = journal.settle(2, &[]);
    assert_eq!(journal.records()[0].lanes.len(), 2);
}

#[test]
fn a_lane_step_is_read_back_by_index() {
    let journal = Journal::default();
    let _ = journal.begin(record(1));
    let _ = journal.advance(1, &lane(1, Progress::CreatingWorktree));
    assert_eq!(journal.step(1, 0), Some(Checkpoint::Planned));
    assert_eq!(journal.step(1, 1), Some(Checkpoint::CreatingWorktree));
    assert_eq!(journal.step(1, 2), None);
    assert_eq!(journal.step(2, 0), None);
}

#[test]
fn a_checkout_or_commit_too_long_to_keep_is_not_recorded() {
    let mut record = record(1);
    record.advance(&lane(0, Progress::CreatingWorktree));
    let mut long = checkout(1);
    long.path = "p".repeat(MAX_FIELD_BYTES + 1);
    assert!(!record.advance(&lane(0, Progress::Created(long))));
    assert_eq!(record.lanes[0].step, Checkpoint::CreatingWorktree);
    assert_eq!(record.lanes[0].checkout, None);
    assert!(!record.advance(&Report::Base("c".repeat(MAX_FIELD_BYTES + 1))));
    assert_eq!(record.commit, None);
    assert!(record.valid());
}

#[test]
fn a_fan_out_with_a_field_too_long_to_keep_is_not_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::at(Some(dir.path().join(FILE)));
    let mut long = record(1);
    long.lanes[0].branch = "b".repeat(MAX_FIELD_BYTES + 1);
    assert!(matches!(
        journal.begin(long),
        Err(Error::JournalFieldTooLong)
    ));
    assert!(journal.records().is_empty());
}

#[test]
fn a_fan_out_is_claimed_by_one_window_only() {
    let journal = Journal::default();
    let _ = journal.begin(record(1));
    // A window's own launch is already claimed.
    assert!(journal.claim(|_| true).is_none());
    let restarted = Journal::default();
    {
        let mut state = restarted.0.state.lock().unwrap();
        state.records = vec![record(2), record(3)];
    }
    assert!(restarted.claim(|_| false).is_none());
    assert_eq!(restarted.claim(|_| true).map(|r| r.id), Some(2));
    assert_eq!(restarted.claim(|_| true).map(|r| r.id), Some(3));
    assert!(restarted.claim(|_| true).is_none());
}

fn saved(records: Vec<Record>) -> Vec<u8> {
    serde_json::to_vec(&Saved { records }).unwrap()
}

#[test]
fn damaged_records_are_dropped_without_the_others() {
    assert_eq!(parse(&saved(vec![record(1)])).unwrap(), vec![record(1)]);
    assert!(matches!(
        parse(b"{\"records\": 3}"),
        Err(Error::JournalJson(_))
    ));
    let mut no_checkout = record(1);
    no_checkout.lanes[0].step = Checkpoint::StartingAgent;
    let mut stray_checkout = record(1);
    stray_checkout.lanes[0].checkout = Some(SavedCheckout::from(&checkout(1)));
    let mut unknown_agent = record(1);
    unknown_agent.lanes[0].kind = "nobody".into();
    let mut long_branch = record(1);
    long_branch.lanes[0].branch = "b".repeat(MAX_FIELD_BYTES + 1);
    let mut no_lanes = record(1);
    no_lanes.lanes.clear();
    for mut bad in [
        no_checkout,
        stray_checkout,
        unknown_agent,
        long_branch,
        no_lanes,
    ] {
        bad.id = 9;
        let parsed = parse(&saved(vec![record(1), bad, record(2)])).unwrap();
        assert_eq!(parsed, vec![record(1), record(2)]);
    }
    let mut repeated = record(1);
    repeated.commit = Some("abc".into());
    assert_eq!(
        parse(&saved(vec![record(1), repeated])).unwrap(),
        vec![record(1)]
    );
    let newest: Vec<u64> = parse(&saved((0..=MAX_RECORDS as u64).map(record).collect()))
        .unwrap()
        .iter()
        .map(|record| record.id)
        .collect();
    assert_eq!(newest, (1..=MAX_RECORDS as u64).collect::<Vec<_>>());
}

#[test]
fn a_damaged_or_oversized_file_is_set_aside_rather_than_saved_over() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let aside = dir.path().join(UNREADABLE);
    fs::write(&path, b"{not json").unwrap();
    let journal = Journal::at(Some(path.clone()));
    assert!(journal.records().is_empty());
    assert_eq!(fs::read(&aside).unwrap(), b"{not json");
    journal.begin(record(1)).unwrap().unwrap().write().unwrap();
    assert_eq!(fs::read(&aside).unwrap(), b"{not json");
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    assert!(Journal::at(Some(path)).records().is_empty());
    assert_eq!(fs::metadata(&aside).unwrap().len(), MAX_FILE_BYTES + 1);
}

#[test]
fn only_the_first_process_to_open_the_journal_saves_it() {
    let dir = tempfile::tempdir().unwrap();
    let first = Journal::locked(dir.path());
    // A second open stands in for another app process: the lock is held
    // per open file, not per process.
    let second = Journal::locked(dir.path());
    assert!(first.begin(record(1)).unwrap().is_some());
    assert!(second.begin(record(2)).unwrap().is_none(), "kept in memory");
    drop(first);
    assert!(
        Journal::locked(dir.path())
            .begin(record(3))
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_snapshot_taken_before_a_rewind_is_retaken_when_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let journal = Journal::at(Some(path.clone()));
    journal.begin(record(1)).unwrap().unwrap().write().unwrap();
    let created = lane(0, Progress::Created(checkout(1)));
    journal.advance(1, &created).unwrap().write().unwrap();
    // Taken while lane 0 claims an agent start whose save then failed.
    let stale = journal
        .advance(1, &lane(0, Progress::StartingAgent))
        .unwrap();
    journal.rewind(1, 0, Checkpoint::Created);
    stale.write().unwrap();
    let records = Journal::at(Some(path)).records();
    assert_eq!(records[0].lanes[0].step, Checkpoint::Created);
}

#[test]
fn the_largest_valid_journal_fits_the_file_limit() {
    let field = "\u{1}".repeat(MAX_FIELD_BYTES);
    let mut largest = Vec::new();
    for id in 0..MAX_RECORDS as u64 {
        let mut record = record(id);
        record.prompt = Some("\u{1}".repeat(MAX_PROMPT_BYTES));
        record.commit = Some(field.clone());
        record.origin = SavedOrigin {
            endpoint_id: field.clone(),
            host: field.clone(),
            endpoint_label: field.clone(),
            workspace_id: field.clone(),
            repo_key: field.clone(),
            repo_label: field.clone(),
            base: field.clone(),
        };
        let lane = SavedLane {
            kind: "claude".into(),
            branch: field.clone(),
            agent: field.clone(),
            step: Checkpoint::Prompted,
            checkout: Some(SavedCheckout {
                endpoint_id: field.clone(),
                host: field.clone(),
                workspace_id: field.clone(),
                path: field.clone(),
                pane_id: field.clone(),
            }),
        };
        record.lanes = vec![lane; MAX_LANES];
        assert!(record.valid());
        largest.push(record);
    }
    assert!(saved(largest).len() as u64 <= MAX_FILE_BYTES);
}

#[test]
fn an_undecodable_record_is_dropped_without_the_others() {
    let mut value: serde_json::Value =
        serde_json::from_slice(&saved(vec![record(1), record(2), record(3)])).unwrap();
    value["records"][1]["lanes"][0]["step"] = "teleported".into();
    value["records"][2]["id"] = "three".into();
    let parsed = parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(parsed, vec![record(1)]);
}

#[test]
fn a_failed_save_is_returned_with_its_cause() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-directory");
    let journal = Journal::at(Some(blocker.join(FILE)));
    // Saving fails from now on: the journal's directory is a file.
    fs::write(&blocker, b"").unwrap();
    let error = journal
        .begin(record(1))
        .unwrap()
        .unwrap()
        .write()
        .unwrap_err();
    assert!(matches!(error, Error::JournalWrite(_)));
    assert!(std::error::Error::source(&error).is_some());
}

mod unsaved;

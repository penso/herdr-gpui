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
    journal.begin(record(1)).unwrap().write();
    journal
        .advance(1, &lane(0, Progress::Created(checkout(1))))
        .unwrap()
        .write();
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
    let older = journal.begin(record(1)).unwrap();
    let newer = journal
        .advance(1, &lane(0, Progress::CreatingWorktree))
        .unwrap();
    newer.write();
    older.write();
    let records = Journal::at(Some(path)).records();
    assert_eq!(records[0].lanes[0].step, Checkpoint::CreatingWorktree);
}

#[test]
fn an_unsaved_journal_has_nothing_to_write() {
    let journal = Journal::default();
    assert!(journal.begin(record(1)).is_none());
    assert_eq!(journal.records().len(), 1);
    assert!(journal.forget(1).is_none());
    assert!(journal.records().is_empty());
}

#[test]
fn the_oldest_fan_out_goes_beyond_the_limit() {
    let journal = Journal::default();
    for id in 0..=MAX_RECORDS as u64 {
        let _ = journal.begin(record(id));
    }
    let ids: Vec<u64> = journal.records().iter().map(|r| r.id).collect();
    assert_eq!(ids, (1..=MAX_RECORDS as u64).collect::<Vec<_>>());
}

#[test]
fn removed_lanes_and_finished_fan_outs_are_dropped() {
    let journal = Journal::default();
    let _ = journal.begin(record(1));
    let _ = journal.drop_lanes(1, &[0]);
    let records = journal.records();
    assert_eq!(records[0].lanes.len(), 1);
    assert_eq!(records[0].lanes[0].kind, "codex");
    let _ = journal.forget(1);
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
fn damaged_journals_are_rejected() {
    assert!(parse(&saved(vec![record(1)])).is_ok());
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
    for bad in [
        vec![no_checkout],
        vec![stray_checkout],
        vec![unknown_agent],
        vec![long_branch],
        vec![no_lanes],
        vec![record(1), record(1)],
        (0..=MAX_RECORDS as u64).map(record).collect(),
    ] {
        assert!(matches!(parse(&saved(bad)), Err(Error::InvalidJournal)));
    }
}

#[test]
fn a_damaged_or_oversized_file_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    std::fs::write(&path, b"{not json").unwrap();
    assert!(Journal::at(Some(path.clone())).records().is_empty());
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    assert!(Journal::at(Some(path)).records().is_empty());
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

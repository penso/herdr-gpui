//! A fan-out the app stopped at each step, restored from its journal on the
//! next launch: what it shows, and what the user may do about it.

use super::*;
use crate::fan_out::journal::Resume;
use std::path::Path;

const ID: u64 = 42;

/// Journal a two-lane launch at `path` whose first lane got through
/// `reports`, as a launch the app quit during would have left it.
fn journal_until(path: &Path, reports: Vec<Report>) {
    let journal = Journal::at(Some(path.to_owned()));
    let lanes = plan::lanes(&launched().picks, 7)[..2].to_vec();
    journal
        .begin(Record::new(ID, &origin(), "fix the bug", &lanes))
        .unwrap()
        .write();
    for report in std::iter::once(Report::Base("c0ffee".into())).chain(reports) {
        if let Some(pending) = journal.advance(ID, &report) {
            pending.write();
        }
    }
}

/// The next launch: a fresh journal from the same file, restored.
fn restart(path: &Path) -> (Journal, Option<FanOut>) {
    let journal = Journal::at(Some(path.to_owned()));
    let restored = FanOut::reclaim(&journal, |_| Some(origin().host));
    (journal, restored)
}

fn lane(progress: Progress) -> Report {
    Report::Lane(0, progress)
}

/// Every step of lane 0, in order, ending with the one in flight.
fn steps(through: usize) -> Vec<Report> {
    let all = vec![
        lane(Progress::CreatingWorktree),
        lane(Progress::Created(checkout(0))),
        lane(Progress::StartingAgent),
        lane(Progress::Prompting),
        lane(Progress::Running),
    ];
    all.into_iter().take(through).collect()
}

/// Restore lane 0 interrupted after `through` steps, checking the fan-out
/// came back as a comparison that started no work of its own.
fn interrupted_after(through: usize) -> FanOut {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    journal_until(&path, steps(through));
    let (_, restored) = restart(&path);
    let mut fan_out = restored.unwrap();
    assert!(matches!(fan_out.stage, Stage::Compare));
    assert!(!fan_out.probing, "restoring reads nothing");
    assert_eq!(fan_out.prompt, "fix the bug");
    assert_eq!(fan_out.base.as_deref(), Some("c0ffee"));
    assert_eq!(fan_out.id, ID);
    // Nothing was sent or removed: no host work reports back.
    assert_eq!(fan_out.poll(), (false, None));
    // The untouched lane is shown, with nothing to do.
    assert_eq!(
        fan_out.lanes[1].state,
        LaneState::Interrupted(Checkpoint::Planned)
    );
    assert_eq!(fan_out.resumable(1), None);
    fan_out
}

#[test]
fn a_fan_out_stopped_before_any_worktree_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    journal_until(&path, Vec::new());
    let (journal, restored) = restart(&path);
    assert!(restored.is_none());
    assert!(journal.records().is_empty());
}

#[test]
fn interrupted_creating_the_worktree_shows_the_branch_may_exist() {
    let fan_out = interrupted_after(1);
    let lane = &fan_out.lanes[0];
    assert_eq!(
        lane.state,
        LaneState::Interrupted(Checkpoint::CreatingWorktree)
    );
    assert!(lane.state.label().contains("branch may exist"));
    // Its workspace is unknown, so nothing is offered or removed.
    assert_eq!(lane.checkout, None);
    assert_eq!(fan_out.resumable(0), None);
    assert!(fan_out.doomed(1).is_empty());
}

#[test]
fn interrupted_after_creating_the_worktree_offers_to_start_the_agent() {
    let fan_out = interrupted_after(2);
    let lane = &fan_out.lanes[0];
    assert_eq!(lane.state, LaneState::Interrupted(Checkpoint::Created));
    assert_eq!(lane.checkout, Some(checkout(0)));
    assert_eq!(fan_out.resumable(0), Some(Resume::Start));
    assert_eq!(fan_out.doomed(1), vec![(0, "w0".to_owned())]);
}

#[test]
fn interrupted_starting_the_agent_only_offers_the_prompt() {
    let fan_out = interrupted_after(3);
    assert_eq!(
        fan_out.lanes[0].state,
        LaneState::Interrupted(Checkpoint::StartingAgent)
    );
    assert_eq!(fan_out.resumable(0), Some(Resume::Prompt));
}

#[test]
fn interrupted_sending_the_prompt_waits_for_the_user_to_resend_it() {
    let fan_out = interrupted_after(4);
    let lane = &fan_out.lanes[0];
    assert_eq!(lane.state, LaneState::Interrupted(Checkpoint::Prompting));
    assert!(lane.state.label().contains("may not have arrived"));
    assert_eq!(fan_out.resumable(0), Some(Resume::Prompt));
    assert_eq!(fan_out.interrupted(), 2);
}

#[test]
fn a_prompted_lane_comes_back_running() {
    let fan_out = interrupted_after(5);
    assert_eq!(fan_out.lanes[0].state, LaneState::Running);
    assert_eq!(fan_out.resumable(0), None);
    assert_eq!(fan_out.interrupted(), 1);
}

#[test]
fn a_restored_fan_out_is_shown_by_one_window_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    journal_until(&path, steps(2));
    let (journal, restored) = restart(&path);
    assert!(restored.is_some());
    assert!(FanOut::reclaim(&journal, |_| Some(origin().host)).is_none());
}

#[test]
fn a_fan_out_on_an_unknown_host_waits_for_a_window_that_knows_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    journal_until(&path, steps(2));
    let journal = Journal::at(Some(path));
    assert!(FanOut::reclaim(&journal, |_| None).is_none());
    assert_eq!(journal.records().len(), 1, "kept for another window");
    assert!(FanOut::reclaim(&journal, |_| Some(origin().host)).is_some());
}

#[test]
fn a_lane_on_a_host_no_longer_set_up_says_where_its_worktree_stays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fan-outs.json");
    journal_until(&path, steps(2));
    let journal = Journal::at(Some(path));
    let mut record = journal.claim(|_| true).unwrap();
    record.lanes[0].checkout.as_mut().unwrap().endpoint_id = "gone".into();
    let host = |endpoint: &str| (endpoint == "local").then(|| origin().host);
    let fan_out = FanOut::restore(record, journal, host).unwrap();
    assert!(
        matches!(&fan_out.lanes[0].state, LaneState::Failed(text) if text.contains("/tmp/lane-0"))
    );
    assert_eq!(fan_out.lanes[0].checkout, None);
    assert_eq!(fan_out.resumable(0), None);
}

#[test]
fn an_unkept_prompt_cannot_be_resent() {
    let journal = Journal::default();
    let lanes = plan::lanes(&launched().picks, 7);
    let mut record = Record::new(ID, &origin(), "go", &lanes);
    record.prompt = None;
    record.advance(&Report::Lane(0, Progress::Created(checkout(0))));
    let fan_out = FanOut::restore(record, journal, |_| Some(origin().host)).unwrap();
    assert_eq!(fan_out.resumable(0), None);
    assert!(fan_out.error.as_deref().unwrap().contains("too long"));
}

#[test]
fn nothing_is_kept_or_forgotten_while_a_lane_is_being_finished() {
    let mut fan_out = interrupted_after(2);
    fan_out.lanes[0].state = LaneState::StartingAgent;
    fan_out.stage = Stage::Confirm(0);
    assert!(!fan_out.keep(0));
    fan_out.stage = Stage::Compare;
    assert!(!fan_out.forget());
    assert_eq!(fan_out.resumable(0), None);
    assert!(!fan_out.resume(0));
}

#[test]
fn forgetting_leaves_the_worktrees_and_drops_the_record() {
    let journal = Journal::default();
    let lanes = plan::lanes(&launched().picks, 7);
    let mut record = Record::new(ID, &origin(), "go", &lanes);
    record.advance(&Report::Lane(0, Progress::Created(checkout(0))));
    let _ = journal.begin(record.clone());
    let mut fan_out = FanOut::restore(record, journal.clone(), |_| Some(origin().host)).unwrap();
    assert!(fan_out.forget());
    assert!(journal.records().is_empty());
    assert_eq!(fan_out.poll(), (false, None), "nothing was removed");
}

#[test]
fn a_launch_that_created_nothing_is_not_journaled_further() {
    let mut fan_out = launched();
    let lanes: Vec<Lane> = fan_out.lanes.iter().map(|view| view.lane.clone()).collect();
    let _ = fan_out
        .journal
        .begin(Record::new(0, &origin(), "go", &lanes));
    send(&fan_out, Event::Launched);
    fan_out.poll();
    assert!(fan_out.journal.records().is_empty());
}

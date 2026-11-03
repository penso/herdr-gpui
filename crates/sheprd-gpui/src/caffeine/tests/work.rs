//! Which agent states keep a window's agents counted as working.
use super::*;
use crate::sidebar::layout_tests::snapshot;
use sheprd_client::protocol::{AgentStatus, ClientShellAgent};

fn with_statuses(statuses: &[AgentStatus]) -> sheprd_client::protocol::ClientShellSnapshot {
    let mut snapshot = snapshot(1);
    let agent = snapshot.agents[0].clone();
    snapshot.agents = statuses
        .iter()
        .map(|status| ClientShellAgent {
            agent_status: *status,
            ..agent.clone()
        })
        .collect();
    snapshot
}

#[test]
fn only_a_working_agent_counts_as_work() {
    use AgentStatus::*;
    let finished = with_statuses(&[Idle, Blocked, Done, Unknown]);
    assert_eq!(Work::of([Some(&finished)]), Work::Idle);
    let busy = with_statuses(&[Done, Working]);
    assert_eq!(Work::of([Some(&finished), Some(&busy)]), Work::Working);
    assert_eq!(Work::of([]), Work::Idle);
}

#[test]
fn an_unreadable_endpoint_is_unknown_unless_another_works() {
    let finished = with_statuses(&[AgentStatus::Done]);
    let busy = with_statuses(&[AgentStatus::Working]);
    assert_eq!(Work::of([Some(&finished), None]), Work::Unknown);
    assert_eq!(Work::of([None, Some(&busy)]), Work::Working);
}

use super::*;
use crate::hub::{Layout, layout::project, recency::AgentTimes};

fn working(pane: &str, tab: &str, seq: u64) -> Value {
    let mut agent = agent(pane, tab, json!([]));
    agent["agent_status"] = json!("working");
    agent["state_change_seq"] = json!(seq);
    agent
}

/// One tab shows only its most pressing agent, so the count comes from
/// every agent: a split tab must not look less busy than it is.
#[test]
fn every_working_agent_counts_even_two_in_one_tab() {
    let mut blocked = agent("p3", "t1", json!([]));
    blocked["agent_status"] = json!("blocked");
    let mut idle = agent("p4", "t2", json!([]));
    idle["agent_status"] = json!("idle");
    let layout = project(&snapshot(json!([
        working("p1", "t1", 1),
        working("p2", "t1", 1),
        blocked,
        idle,
    ])));
    assert_eq!(layout.working, 2);
}

#[test]
fn a_terminal_agent_is_dated_once_it_moves() {
    let mut times = AgentTimes::default();
    let first = snapshot(json!([working("p1", "t1", 1)]));
    let mut layout = project(&first);
    times.stamp(&first, &mut layout, 100);
    let agent_of = |layout: &Layout| {
        layout.workspaces[0].terminals[0]
            .agent
            .as_ref()
            .and_then(|agent| agent.changed_ms)
    };
    assert_eq!(agent_of(&layout), None);

    let next = snapshot(json!([working("p1", "t1", 2)]));
    let mut layout = project(&next);
    times.stamp(&next, &mut layout, 200);
    assert_eq!(agent_of(&layout), Some(200));
}

#![allow(clippy::unwrap_used)]
use super::{Groups, group_label, is_coordinator};
use crate::sidebar::layout_tests::snapshot;
use sheprd_client::protocol::{ClientShellAgent, ClientShellPane, ClientShellSnapshot};

const SUFFIX: &str = "-coord";

/// A snapshot of one agent per name, panes `p0`, `p1`, and so on.
fn sessions(names: &[&str]) -> ClientShellSnapshot {
    let mut snapshot = snapshot(1);
    let agent = snapshot.agents[0].clone();
    snapshot.agents = names
        .iter()
        .enumerate()
        .map(|(index, name)| ClientShellAgent {
            pane_id: format!("p{index}"),
            name: Some((*name).to_owned()),
            ..agent.clone()
        })
        .collect();
    snapshot
}

/// A pane with a Rename label.
fn pane(id: &str, label: &str) -> ClientShellPane {
    serde_json::from_value(serde_json::json!({
        "pane_id": id, "workspace_id": "w0", "tab_id": "t0", "label": label,
        "cwd": null, "foreground_cwd": null, "focused": false,
        "right_click_passthrough": false,
    }))
    .unwrap()
}

fn groups<'a>(snapshot: &'a ClientShellSnapshot, selected: Option<&'a str>) -> Groups<'a> {
    Groups::new(
        SUFFIX,
        snapshot.agents.iter().map(|agent| (agent, snapshot)),
        selected,
    )
}

/// The names `group` lists, in its order.
fn listed<'a>(snapshot: &'a ClientShellSnapshot, group: &'a str) -> Vec<&'a str> {
    groups(snapshot, Some(group))
        .narrow(
            group,
            snapshot.agents.iter().map(|agent| (0, agent, snapshot)),
        )
        .into_iter()
        .filter_map(|(_, agent)| agent.name.as_deref())
        .collect()
}

#[test]
fn coordinators_name_the_groups_in_a_stable_order() {
    let snapshot = sessions(&["zed-coord", "demo-12", "demo-coord", "demo-coord", "-coord"]);
    assert_eq!(groups(&snapshot, None).names(), ["demo", "zed"]);
}

#[test]
fn a_group_leads_with_its_coordinator_and_keeps_the_rest_in_order() {
    let snapshot = sessions(&["demo-13", "other-1", "demo-coord", "demo-12", "other-coord"]);
    assert_eq!(
        listed(&snapshot, "demo"),
        ["demo-coord", "demo-13", "demo-12"]
    );
    assert_eq!(listed(&snapshot, "other"), ["other-coord", "other-1"]);
}

#[test]
fn a_worker_needs_a_separator_after_the_group() {
    let snapshot = sessions(&["demo-coord", "demolition-1", "demo", "demo_2", "demo/3"]);
    assert_eq!(
        listed(&snapshot, "demo"),
        ["demo-coord", "demo_2", "demo/3"]
    );
}

#[test]
fn the_longest_group_claims_a_worker() {
    let snapshot = sessions(&["demo-coord", "demo-api-coord", "demo-api-12", "demo-12"]);
    assert_eq!(listed(&snapshot, "demo"), ["demo-coord", "demo-12"]);
    assert_eq!(
        listed(&snapshot, "demo-api"),
        ["demo-api-coord", "demo-api-12"]
    );
}

/// Membership lives in names, not panes: a new pane given the name takes the
/// group over, and the workers never leave the view in between.
#[test]
fn a_replaced_coordinator_keeps_its_group() {
    let before = sessions(&["demo-coord", "demo-12"]);
    assert_eq!(listed(&before, "demo"), ["demo-coord", "demo-12"]);

    let gone = sessions(&["demo-12"]);
    assert!(groups(&gone, None).names().is_empty());
    assert_eq!(groups(&gone, Some("demo")).names(), ["demo"]);
    assert_eq!(listed(&gone, "demo"), ["demo-12"]);

    let mut replaced = sessions(&["demo-12", "demo-coord"]);
    replaced.agents[1].pane_id = "new-pane".into();
    assert_eq!(listed(&replaced, "demo"), ["demo-coord", "demo-12"]);
}

#[test]
fn a_pane_label_names_a_session_the_agent_name_does_not() {
    let mut snapshot = sessions(&["claude", "codex"]);
    snapshot.panes = vec![pane("p0", " demo-coord "), pane("p1", "demo-12")];
    assert!(is_coordinator(&snapshot.agents[0], &snapshot, SUFFIX));
    assert!(!is_coordinator(&snapshot.agents[1], &snapshot, SUFFIX));
    assert_eq!(listed(&snapshot, "demo"), ["claude", "codex"]);
}

/// A group is only offered if its coordinator would be listed in it, so the
/// name that places a session is the one that forms a group.
#[test]
fn a_session_with_two_coordinator_names_forms_only_the_group_it_joins() {
    let mut snapshot = sessions(&["demo-coord", "other-1"]);
    snapshot.panes = vec![pane("p0", "other-coord")];
    assert_eq!(groups(&snapshot, None).names(), ["demo"]);
    assert_eq!(listed(&snapshot, "demo"), ["demo-coord"]);
}

#[test]
fn the_header_steps_through_each_group_then_back_to_all() {
    let snapshot = sessions(&["a-coord", "b-coord"]);
    let groups = groups(&snapshot, None);
    assert_eq!(groups.after(None), Some("a"));
    assert_eq!(groups.after(Some("a")), Some("b"));
    assert_eq!(groups.after(Some("b")), None);
}

#[test]
fn a_hostile_group_name_shows_as_safe_bounded_text() {
    assert_eq!(group_label("\u{1b}[31mdemo\u{202e}"), "[31mdemo");
    assert_eq!(group_label("\u{202e}"), "?");
    assert!(group_label(&"x".repeat(10_000)).chars().count() <= 160);
}

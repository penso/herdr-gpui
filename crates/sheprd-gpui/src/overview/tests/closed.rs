use super::*;
use crate::{
    overview::{
        closed::{self, Closed},
        reopen,
    },
    sidebar::layout_tests::snapshot,
    teleport::AgentKind,
};
use sheprd_client::protocol::{ClientShellPane, ClientShellSnapshot};

/// A report of host `local` under `boot`, with a pane for each agent.
fn report(boot: &str, agents: &[&str]) -> (ClientShellSnapshot, Vec<Row>) {
    let mut shown = snapshot(1);
    shown.boot_id = boot.into();
    shown.panes = agents
        .iter()
        .map(|agent| ClientShellPane {
            pane_id: format!("pane-{agent}"),
            workspace_id: "w0".into(),
            tab_id: "t0".into(),
            label: None,
            cwd: Some("/src/herdr".into()),
            foreground_cwd: Some(format!("/src/herdr/{agent}")),
            focused: false,
            right_click_passthrough: false,
        })
        .collect();
    let rows = agents
        .iter()
        .map(|agent| row(agent, State::Working))
        .collect();
    (shown, rows)
}

fn observe(closed: &mut Closed, boot: &str, agents: &[&str], now: u64) {
    let (shown, rows) = report(boot, agents);
    closed.observe("local", &shown, &rows, at(now));
}

/// The closed agent shown as `name`.
fn named<'a>(closed: &'a Closed, name: &str) -> &'a closed::Agent {
    closed
        .agents()
        .find(|agent| agent.row.detail == name)
        .unwrap()
}

fn names(closed: &Closed) -> Vec<String> {
    closed
        .agents()
        .map(|agent| agent.row.detail.clone())
        .collect()
}

#[test]
fn an_agent_gone_from_its_host_is_closed_with_its_folder() {
    let mut closed = Closed::default();
    observe(&mut closed, "b1", &["a", "b"], 10);
    // The first report closes nothing: there was nothing before it.
    assert!(closed.is_empty());
    let revision = closed.revision();
    observe(&mut closed, "b1", &["b"], 20);
    assert_eq!(names(&closed), ["a"]);
    assert!(closed.revision() > revision);
    let agent = named(&closed, "a");
    assert_eq!(agent.closed, at(20));
    assert_eq!(agent.cwd.as_deref(), Some("/src/herdr/a"));
    // Newest first.
    observe(&mut closed, "b1", &[], 30);
    assert_eq!(names(&closed), ["b", "a"]);
}

#[test]
fn an_agent_replaced_in_its_pane_is_closed() {
    let mut closed = Closed::default();
    let (shown, rows) = report("b1", &["a"]);
    closed.observe("local", &shown, &rows, at(10));
    let mut codex = rows[0].clone();
    codex.identity = Some("codex".into());
    closed.observe("local", &shown, [&codex], at(20));
    assert_eq!(names(&closed), ["a"]);
    assert_eq!(
        closed.agents().next().unwrap().row.identity.as_deref(),
        Some("claude")
    );
}

#[test]
fn a_daemon_restart_closes_nothing() {
    let mut closed = Closed::default();
    observe(&mut closed, "b1", &["a"], 10);
    observe(&mut closed, "b2", &[], 20);
    assert!(closed.is_empty());
}

#[test]
fn an_agent_that_comes_back_is_no_longer_closed() {
    let mut closed = Closed::default();
    observe(&mut closed, "b1", &["a"], 10);
    observe(&mut closed, "b1", &[], 20);
    observe(&mut closed, "b1", &["a"], 30);
    assert!(closed.is_empty());
}

#[test]
fn a_twin_still_running_keeps_its_closed_namesake_listed() {
    let mut closed = Closed::default();
    let (mut shown, mut rows) = report("b1", &["a"]);
    let mut twin = rows[0].clone();
    if let Open::Pane { pane, .. } = &mut twin.open {
        *pane = "pane-twin".into();
    }
    rows.push(twin.clone());
    closed.observe("local", &shown, &rows, at(10));
    shown.panes.truncate(0);
    closed.observe("local", &shown, [&twin], at(20));
    closed.observe("local", &shown, [&twin], at(30));
    assert_eq!(names(&closed), ["a"]);
}

/// Two agents sharing `a`'s key, in their own panes and folders.
fn twins() -> (ClientShellSnapshot, Vec<Row>) {
    let (mut shown, mut rows) = report("b1", &["a"]);
    let mut pane = shown.panes[0].clone();
    pane.pane_id = "pane-twin".into();
    pane.foreground_cwd = Some("/src/herdr/twin".into());
    shown.panes.push(pane);
    let mut twin = rows[0].clone();
    if let Open::Pane { pane, .. } = &mut twin.open {
        *pane = "pane-twin".into();
    }
    rows.push(twin);
    (shown, rows)
}

#[test]
fn twins_that_both_close_are_both_listed() {
    let mut closed = Closed::default();
    let (shown, rows) = twins();
    closed.observe("local", &shown, &rows, at(10));
    closed.observe("local", &shown, [&rows[1]], at(20));
    closed.observe("local", &shown, &[], at(30));
    let cwds: Vec<_> = closed
        .agents()
        .map(|agent| agent.cwd.as_deref().unwrap())
        .collect();
    assert_eq!(cwds, ["/src/herdr/twin", "/src/herdr/a"]);
    let twin = closed.agents().next().unwrap().id;
    assert_ne!(twin, closed.agents().nth(1).unwrap().id);
    closed.forget(twin);
    assert_eq!(names(&closed), ["a"]);
    assert_eq!(closed.get(twin), None);
    // Each agent of the key coming back takes back one closed agent.
    let (shown, rows) = twins();
    closed.observe("local", &shown, &rows, at(40));
    assert!(closed.is_empty());
    closed.observe("local", &shown, &[], at(50));
    assert_eq!(closed.len(), 2);
    closed.observe("local", &shown, [&rows[0]], at(60));
    assert_eq!(closed.len(), 1);
}

#[test]
fn reopening_one_twin_keeps_the_other_closed() {
    let mut closed = Closed::default();
    let (shown, rows) = twins();
    closed.observe("local", &shown, &rows, at(10));
    closed.observe("local", &shown, &[], at(20));
    let first = closed.agents().nth(1).unwrap().id;
    closed.reopened(first);
    assert_eq!(closed.len(), 1);
    // The reopened agent turns up: the other twin stays closed.
    closed.observe("local", &shown, [&rows[0]], at(30));
    assert_eq!(closed.len(), 1);
    // A second one of the key is a twin back.
    closed.observe("local", &shown, &rows, at(40));
    assert!(closed.is_empty());
}

#[test]
fn forgetting_and_dropped_hosts_clear_closed_agents() {
    let mut closed = Closed::default();
    observe(&mut closed, "b1", &["a", "b"], 10);
    observe(&mut closed, "b1", &[], 20);
    closed.forget(named(&closed, "a").id);
    assert_eq!(names(&closed), ["b"]);
    closed.retain_hosts(|id| id != "local");
    assert!(closed.is_empty());
}

#[test]
fn closed_agents_are_bounded() {
    let mut closed = Closed::default();
    let agents: Vec<String> = (0..40).map(|index| format!("agent-{index}")).collect();
    let all: Vec<&str> = agents.iter().map(String::as_str).collect();
    observe(&mut closed, "b1", &all, 10);
    observe(&mut closed, "b1", &[], 20);
    assert!(closed.len() < all.len());
    assert!(!closed.is_empty());
}

#[test]
fn a_reopened_agent_continues_where_it_can() {
    assert_eq!(reopen::command_line(AgentKind::Claude), "claude --continue");
    assert_eq!(
        reopen::command_line(AgentKind::Codex),
        "codex resume --last"
    );
    assert_eq!(
        reopen::command_line(AgentKind::Opencode),
        "opencode --continue"
    );
    // An agent without a known way to continue starts afresh.
    assert_eq!(reopen::command_line(AgentKind::Cursor), "cursor-agent");
}

#[test]
fn only_known_agents_can_be_reopened() {
    let mut closed = Closed::default();
    observe(&mut closed, "b1", &["a"], 10);
    observe(&mut closed, "b1", &[], 20);
    let agent = named(&closed, "a").clone();
    assert_eq!(reopen::kind(&agent), Some(AgentKind::Claude));
    let mut unknown = agent;
    unknown.row.identity = Some("make".into());
    assert_eq!(reopen::kind(&unknown), None);
}

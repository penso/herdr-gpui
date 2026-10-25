use super::*;
use crate::hub::layout::{MAX_CHATS, project};
use herdr_client::protocol::AgentStatus;

const ENDPOINT: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn chat_panes_and_terminal_tabs_are_listed_per_workspace() {
    let layout = project(&snapshot(json!([agent(
        "p1",
        "t1",
        json!([["acp", ENDPOINT]])
    )])));
    assert!(layout.connected);
    assert_eq!(layout.boot, "boot-1");
    let workspace = &layout.workspaces[0];
    assert_eq!(workspace.label, "app");
    assert_eq!(workspace.cwd, "/code/app");
    assert_eq!(workspace.chats.len(), 1);
    assert_eq!(workspace.chats[0].pane, "p1");
    assert_eq!(workspace.chats[0].title, "Remote plan");
    assert_eq!(workspace.chats[0].endpoint.as_str(), ENDPOINT);
    // The chat's own tab is not a terminal.
    assert_eq!(workspace.terminals.len(), 1);
    assert_eq!(workspace.terminals[0].label, "zsh");
    assert_eq!(workspace.terminals[0].cwd.as_deref(), Some("/code/app/src"));
    // It lists its panes, and a phone opens the first when none is focused.
    let terminal = &workspace.terminals[0];
    assert_eq!(terminal.panes.len(), 1);
    assert_eq!(terminal.first_pane().unwrap().pane, "p2");
    assert_eq!(layout.terminal("p2").unwrap().1.tab, "t2");
    assert!(
        layout.terminal("p1").is_none(),
        "a chat's pane is no terminal"
    );
}

#[test]
fn an_agent_without_a_valid_endpoint_is_not_a_chat() {
    let layout = project(&snapshot(json!([
        agent("p1", "t1", json!([])),
        agent("p2", "t2", json!([["acp", "../../etc/passwd"]])),
    ])));
    assert!(layout.workspaces[0].chats.is_empty());
    assert_eq!(layout.workspaces[0].terminals.len(), 2);
}

#[test]
fn a_pane_is_one_chat_however_often_it_is_reported() {
    let tokens = json!([["acp", ENDPOINT]]);
    let layout = project(&snapshot(json!([
        agent("p1", "t1", tokens.clone()),
        agent("p1", "t1", tokens),
    ])));
    assert_eq!(layout.workspaces[0].chats.len(), 1);
}

#[test]
fn chats_are_bounded() {
    let agents: Vec<Value> = (0..MAX_CHATS + 5)
        .map(|index| agent(&format!("p{index}"), "t1", json!([["acp", ENDPOINT]])))
        .collect();
    let layout = project(&snapshot(Value::Array(agents)));
    assert_eq!(layout.chats().count(), MAX_CHATS);
}

#[test]
fn a_chat_is_found_with_its_workspace() {
    let layout = project(&snapshot(json!([agent(
        "p1",
        "t1",
        json!([["acp", ENDPOINT]])
    )])));
    let (workspace, chat) = layout.chat("p1").unwrap();
    assert_eq!(workspace.id, "w1");
    assert_eq!(chat.tab, "t1");
    assert!(layout.chat("p9").is_none());
}

#[test]
fn a_terminal_tab_shows_the_agent_herdr_detects_in_it() {
    let mut idle = agent("p2", "t2", json!([]));
    idle["agent_status"] = json!("idle");
    let mut blocked = agent("p9", "t2", json!([]));
    blocked["agent_status"] = json!("blocked");
    let layout = project(&snapshot(json!([idle, blocked])));
    let (_, terminal) = layout.terminal("p2").unwrap();
    // Several agents in one tab: the most pressing one shows.
    assert_eq!(
        terminal.agent,
        Some((Some("claude".to_owned()), AgentStatus::Blocked))
    );
    let plain = project(&snapshot(json!([])));
    assert_eq!(plain.terminal("p2").unwrap().1.agent, None);
}

#[test]
fn an_acp_chat_shows_the_agent_it_runs_not_acp() {
    let mut chat = agent("p1", "t1", json!([["acp", ENDPOINT]]));
    chat["agent"] = json!("acp");
    chat["display_agent"] = json!("Claude");
    let layout = project(&snapshot(json!([chat])));
    assert_eq!(
        layout.workspaces[0].chats[0].agent.as_deref(),
        Some("Claude")
    );
}

//! The badge's count: who needs an answer, and who finished unseen.

use super::*;
use crate::{
    hub::{ChatRow, Terminal, TerminalAgent, Workspace},
    web::attention::Opened,
};
use sheprd_chat_protocol::EndpointId;
use sheprd_client::protocol::AgentStatus;
use std::collections::BTreeSet;

fn chat(pane: &str, status: AgentStatus) -> ChatRow {
    ChatRow {
        pane: pane.to_owned(),
        tab: format!("tab-{pane}"),
        endpoint: EndpointId::random(),
        agent: None,
        title: String::new(),
        status,
    }
}

fn terminal(pane: &str, status: AgentStatus) -> Terminal {
    Terminal {
        tab: format!("tab-{pane}"),
        label: String::new(),
        cwd: None,
        panes: Vec::new(),
        zoomed: false,
        agent: Some(TerminalAgent {
            name: None,
            status,
            pane: pane.to_owned(),
            title: None,
            changed_ms: None,
        }),
    }
}

fn layout(chats: Vec<ChatRow>, terminals: Vec<Terminal>) -> Layout {
    Layout {
        workspaces: vec![Workspace {
            id: "w1".to_owned(),
            label: "work".to_owned(),
            branch: None,
            cwd: "/work".to_owned(),
            worktree: None,
            chats,
            terminals,
        }],
        ..Layout::default()
    }
}

fn panes<const N: usize>(names: [&str; N]) -> BTreeSet<String> {
    names.into_iter().map(str::to_owned).collect()
}

fn no_questions(_: &str) -> usize {
    0
}

#[test]
fn finished_blocked_and_asking_agents_count_once_each() {
    let layout = layout(
        vec![
            chat("done", AgentStatus::Done),
            chat("asking", AgentStatus::Working),
            chat("both", AgentStatus::Blocked),
            chat("working", AgentStatus::Working),
            chat("idle", AgentStatus::Idle),
        ],
        vec![
            terminal("term-done", AgentStatus::Done),
            terminal("term-blocked", AgentStatus::Blocked),
            terminal("term-unknown", AgentStatus::Unknown),
        ],
    );
    let questions = |pane: &str| match pane {
        "asking" => 1,
        "both" => 3,
        _ => 0,
    };
    assert_eq!(
        Opened::default().needing(&layout, questions),
        panes(["asking", "both", "done", "term-blocked", "term-done"])
    );
}

#[test]
fn opening_a_finished_agent_stops_it_counting_until_it_finishes_again() {
    let opened = Opened::default();
    let done = layout(vec![chat("p", AgentStatus::Done)], Vec::new());
    assert_eq!(opened.needing(&done, no_questions), panes(["p"]));

    opened.open(&done, "p");
    assert_eq!(opened.needing(&done, no_questions), panes([]));

    let working = layout(vec![chat("p", AgentStatus::Working)], Vec::new());
    assert_eq!(opened.needing(&working, no_questions), panes([]));
    assert_eq!(opened.needing(&done, no_questions), panes(["p"]));
}

#[test]
fn opening_an_agent_still_working_does_not_hide_its_finish() {
    let opened = Opened::default();
    let working = layout(vec![chat("p", AgentStatus::Working)], Vec::new());
    opened.open(&working, "p");
    let done = layout(vec![chat("p", AgentStatus::Done)], Vec::new());
    assert_eq!(opened.needing(&done, no_questions), panes(["p"]));
}

#[test]
fn opening_does_not_hide_a_question() {
    let opened = Opened::default();
    let blocked = layout(vec![chat("p", AgentStatus::Blocked)], Vec::new());
    opened.open(&blocked, "p");
    assert_eq!(opened.needing(&blocked, no_questions), panes(["p"]));
}

#[test]
fn opening_tells_views_of_the_count_once() {
    let opened = Opened::default();
    let mut changed = opened.changed();
    let done = layout(vec![chat("p", AgentStatus::Done)], Vec::new());
    opened.open(&done, "p");
    assert!(changed.has_changed().unwrap());
    changed.mark_unchanged();
    opened.open(&done, "p");
    assert!(!changed.has_changed().unwrap());
}

#[test]
fn an_agent_that_finishes_in_an_open_view_was_seen() {
    let opened = Opened::default();
    let working = layout(vec![chat("p", AgentStatus::Working)], Vec::new());
    let done = layout(vec![chat("p", AgentStatus::Done)], Vec::new());
    opened.begin(&working, "p");
    assert_eq!(opened.needing(&done, no_questions), panes([]));
    opened.end(&done, "p");
    assert_eq!(opened.needing(&done, no_questions), panes([]));
}

#[test]
fn a_finish_nothing_asked_about_while_the_view_was_open_was_seen() {
    let opened = Opened::default();
    let working = layout(vec![chat("p", AgentStatus::Working)], Vec::new());
    let done = layout(vec![chat("p", AgentStatus::Done)], Vec::new());
    opened.begin(&working, "p");
    opened.end(&done, "p");
    assert_eq!(opened.needing(&done, no_questions), panes([]));
}

#[test]
fn an_agent_that_finishes_after_its_last_view_closed_counts() {
    let opened = Opened::default();
    let working = layout(vec![chat("p", AgentStatus::Working)], Vec::new());
    let done = layout(vec![chat("p", AgentStatus::Done)], Vec::new());
    opened.begin(&working, "p");
    opened.begin(&working, "p");
    opened.end(&working, "p");
    assert_eq!(opened.needing(&done, no_questions), panes([]));
    opened.end(&done, "p");
    let again = Opened::default();
    again.begin(&working, "p");
    again.end(&working, "p");
    assert_eq!(again.needing(&done, no_questions), panes(["p"]));
}

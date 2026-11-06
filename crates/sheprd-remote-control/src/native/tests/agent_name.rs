//! A chat's agent, for its logo: Herdr may know an ACP chat only as
//! `acp`, so the chat's own transcript names it once linked.

use super::*;
use crate::control::pull_requests::PullRequests;
use model::{Home, Sources};

fn home_with(agent: Option<&str>, summary: Option<&str>) -> Home {
    let mut layout = layout(EndpointId::random());
    layout.workspaces[0].chats[0].agent = agent.map(str::to_owned);
    let summaries = summary
        .map(|agent| {
            let summary = Summary {
                agent: agent.to_owned(),
                ..Summary::default()
            };
            ("p1".to_owned(), summary)
        })
        .into_iter()
        .collect();
    Home::project(Sources {
        computer: "Studio",
        fingerprint: "FP",
        layout: &layout,
        load: None,
        summaries,
        pull_requests: &PullRequests::default(),
    })
}

fn agent(home: &Home) -> Option<&str> {
    home.workspaces[0].chats[0].agent.as_deref()
}

#[test]
fn a_linked_acp_chat_takes_its_agent_from_the_transcript() {
    assert_eq!(
        agent(&home_with(Some("acp"), Some("Claude"))),
        Some("Claude")
    );
}

#[test]
fn a_chat_not_linked_yet_keeps_herdrs_name() {
    assert_eq!(agent(&home_with(Some("acp"), None)), Some("acp"));
    assert_eq!(agent(&home_with(Some("claude"), None)), Some("claude"));
}

#[test]
fn a_transcript_without_an_agent_keeps_herdrs_name() {
    assert_eq!(agent(&home_with(Some("claude"), Some(" "))), Some("claude"));
}

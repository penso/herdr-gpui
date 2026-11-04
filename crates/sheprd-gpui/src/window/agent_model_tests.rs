//! The focused agent's model at the start of the status bar: its label, and
//! which agents it opens a model picker for.

#![allow(clippy::unwrap_used)]

use super::{
    MockPeer, SheprdWindow,
    status_bar::{has_model_picker, model_label},
};
use crate::{
    NavigationTarget,
    sidebar::layout_tests::{fixture_window, full_draw},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use sheprd_client::protocol::{ClientMessage, ClientPaneInputEvent, ClientShellAgent};
use std::sync::Arc;

fn agent(kind: &str, tokens: &[(&str, &str)]) -> ClientShellAgent {
    let mut agent = crate::sidebar::layout_tests::snapshot(1).agents.remove(0);
    agent.agent = Some(kind.into());
    agent.tokens = tokens
        .iter()
        .map(|(name, value)| ((*name).into(), (*value).into()))
        .collect();
    agent
}

#[test]
fn the_label_is_the_model_then_the_effort() {
    let agent = agent("claude", &[("model", "Opus 5.5"), ("effort", "high")]);
    assert_eq!(model_label(&agent).as_deref(), Some("Opus 5.5 \u{b7} high"));
}

#[test]
fn the_label_is_the_model_alone_without_an_effort() {
    let agent = agent("codex", &[("model", "gpt-5")]);
    assert_eq!(model_label(&agent).as_deref(), Some("gpt-5"));
}

#[test]
fn there_is_no_label_without_a_model() {
    let agent = agent("claude", &[("effort", "high"), ("pr", "#12")]);
    assert_eq!(model_label(&agent), None);
}

/// `/model` is typed only into Claude: another agent could take the text
/// as a prompt.
#[test]
fn only_claude_gets_the_model_picker() {
    assert!(has_model_picker(&agent("claude", &[])));
    for other in ["codex", "opencode", "pi"] {
        assert!(!has_model_picker(&agent(other, &[])));
    }
    let mut unknown = agent("claude", &[]);
    unknown.agent = None;
    assert!(!has_model_picker(&unknown));
}

fn set_first_agent(
    view: &gpui::Entity<SheprdWindow>,
    focused: bool,
    tokens: &[(&str, &str)],
    cx: &mut VisualTestContext,
) {
    let tokens: Vec<(String, String)> = tokens
        .iter()
        .map(|(name, value)| ((*name).into(), (*value).into()))
        .collect();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[0].focused = focused;
            snapshot.agents[0].tokens = tokens;
            cx.notify();
        });
    });
}

fn model_segment_drawn(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.debug_bounds("agent-model").is_some()
}

#[gpui::test]
fn the_bar_shows_the_focused_agents_model_only(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    assert!(!model_segment_drawn(cx));

    set_first_agent(&view, true, &[("model", "Opus 5.5")], cx);
    assert!(model_segment_drawn(cx));

    set_first_agent(&view, false, &[("model", "Opus 5.5")], cx);
    assert!(!model_segment_drawn(cx));

    set_first_agent(&view, true, &[], cx);
    assert!(!model_segment_drawn(cx));
}

fn set_first_agent_kind(view: &gpui::Entity<SheprdWindow>, kind: &str, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[0].agent = Some(kind.into());
            cx.notify();
        });
    });
}

fn click_model(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let segment = cx.debug_bounds("agent-model").unwrap();
    cx.simulate_click(segment.center(), Modifiers::default());
}

/// The next input typed into a pane, skipping the resize and focus traffic
/// a drawn window sends on its own.
fn next_pane_input(peer: &mut MockPeer) -> ClientMessage {
    loop {
        let message = peer.receive();
        if matches!(message, ClientMessage::ClientShellPaneInput { .. }) {
            return message;
        }
    }
}

fn typed(events: Vec<ClientPaneInputEvent>) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events,
    }
}

#[gpui::test]
fn a_click_types_model_into_claude_only(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| view.update(cx, |view, cx| peer.prepare(view, cx)));
    set_first_agent(&view, true, &[("model", "Opus 5.5")], cx);

    click_model(cx);
    assert_eq!(
        next_pane_input(&mut peer),
        typed(vec![
            ClientPaneInputEvent::TextCommit("/model".into()),
            crate::menu::enter_key(),
        ])
    );
    // The pane gets the focus, here once the window can navigate.
    let focus =
        |cx: &mut VisualTestContext| view.update(cx, |view, _| view.pending_navigation.take());
    assert_eq!(focus(cx), Some(NavigationTarget::Pane("w1:p1".into())));

    // Another agent's click sends nothing and moves no focus: the next input
    // is one sent after it.
    set_first_agent_kind(&view, "codex", cx);
    click_model(cx);
    assert_eq!(focus(cx), None);
    let sentinel = ClientPaneInputEvent::TextCommit("sentinel".into());
    peer.client
        .handle
        .send_input("boot-v1", "w1:p1", [sentinel.clone()])
        .unwrap();
    assert_eq!(next_pane_input(&mut peer), typed(vec![sentinel]));
}

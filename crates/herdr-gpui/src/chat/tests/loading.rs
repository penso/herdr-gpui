//! Delayed attachments keep the real terminal's editor and keyboard path.

use super::*;
use crate::{
    HerdrWindow, chat::terminal::InputState, sidebar::layout_tests::full_draw, window::MockPeer,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use herdr_client::{
    ConnectTarget,
    protocol::{AgentStatus, ClientPaneInputEvent},
};

fn pending(
    cx: &mut TestAppContext,
) -> (
    Entity<HerdrWindow>,
    MockPeer,
    Program,
    Peer,
    &mut VisualTestContext,
) {
    let (view, peer, cx) = overlay::open(cx, ConnectTarget::Local);
    let program = Program::new();
    overlay::poll(&view, &program, cx);
    let (mut stream, _) = program.accept();
    send(&mut stream, &welcome());
    cx.simulate_resize(size(px(1100.), px(900.)));
    overlay::poll(&view, &program, cx);
    selection::acknowledge_size(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                AgentStatus::Idle;
            Arc::make_mut(view.live.surface.as_mut().unwrap())
                .frame
                .cursor = None;
            window.focus(&view.focus, cx);
        });
        full_draw(window, cx).clear(cx);
    });
    overlay::poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_some());
    assert!(cx.debug_bounds("chat-view").is_none());
    assert!(!view.read_with(cx, |view, _| view.chats.shown("w1:p1")));
    (view, peer, program, stream, cx)
}

#[gpui::test]
fn a_delayed_snapshot_keeps_terminal_input_and_then_replaces_the_loading_shell(
    cx: &mut TestAppContext,
) {
    let (view, mut peer, program, mut stream, cx) = pending(cx);
    assert!(cx.update(|window, cx| view.read(cx).focus.is_focused(window)));
    cx.simulate_input("still editing");
    // Native input may commit one character at a time. Check its exact order
    // independently of batching; a following sentinel rejects duplicate input.
    let mut typed = String::new();
    while typed.len() < "still editing".len() {
        for event in terminal_input::next_input(&mut peer) {
            let ClientPaneInputEvent::TextCommit(text) = event else {
                panic!("unexpected input during loading: {event:?}");
            };
            typed.push_str(&text);
        }
    }
    assert_eq!(typed, "still editing");
    cx.simulate_input("!");
    assert_eq!(
        terminal_input::next_input(&mut peer),
        [ClientPaneInputEvent::TextCommit("!".into())]
    );
    view.update(cx, |view, _| {
        assert!(
            view.chats
                .pane_mut("w1:p1")
                .unwrap()
                .session
                .resend()
                .is_empty()
        );
    });
    send(
        &mut stream,
        &snapshot(cursor(1, 0), vec![agent(1, "Ready.")]),
    );
    wait_until(|| {
        overlay::poll(&view, &program, cx);
        cx.debug_bounds("chat-view").is_some()
    });
    assert!(cx.debug_bounds("chat-loading").is_none());
}

#[gpui::test]
fn loading_opt_out_persists_when_the_first_snapshot_arrives(cx: &mut TestAppContext) {
    let (view, _peer, program, mut stream, cx) = pending(cx);
    let terminal = cx.debug_bounds("chat-loading-terminal").unwrap();
    cx.simulate_click(terminal.center(), Modifiers::default());
    overlay::poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_none());
    assert!(view.read_with(cx, |view, _| view.chats.raw("w1:p1")));
    send(
        &mut stream,
        &snapshot(cursor(1, 0), vec![agent(1, "Ready.")]),
    );
    wait_until(|| {
        overlay::poll(&view, &program, cx);
        cx.debug_bounds("chat-switch").is_some()
    });
    assert!(cx.debug_bounds("chat-view").is_none());
}

#[gpui::test]
fn blocked_or_failed_attachments_reveal_the_original_terminal(cx: &mut TestAppContext) {
    let (view, _peer, program, mut stream, cx) = pending(cx);
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
            AgentStatus::Blocked;
    });
    overlay::poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_none());
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
            AgentStatus::Idle;
    });
    overlay::poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_some());
    send(
        &mut stream,
        &ServerMessage::Bye {
            reason: ByeReason::Exiting,
        },
    );
    wait_until(|| {
        overlay::poll(&view, &program, cx);
        !view.read_with(cx, |view, _| view.chats.pane("w1:p1").unwrap().has_link())
    });
    assert!(cx.debug_bounds("chat-loading").is_none());
    assert!(cx.debug_bounds("chat-view").is_none());
}

#[gpui::test]
fn a_blocked_editor_stays_uncovered_even_when_its_geometry_awaits_fit(cx: &mut TestAppContext) {
    let (view, _peer, program, _stream, cx) = pending(cx);
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
            AgentStatus::Blocked;
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.frame.height = view.options.surface_size.rows + 8;
        surface.panes[0].rect.height = surface.frame.height;
        surface.panes[0].inner_rect.height = surface.frame.height;
        let surface = view.live.surface.as_ref().unwrap();
        let layout = view.existing_terminal_layout(&surface.panes[0], surface, Default::default());
        assert_eq!(layout.state, InputState::AwaitingFit);
        assert!(layout.conversation.height > 0);
    });
    overlay::poll(&view, &program, cx);
    assert!(view.read_with(cx, |view, _| view.chats.pane("w1:p1").unwrap().loading()));
    assert!(cx.debug_bounds("chat-loading").is_none());
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
            AgentStatus::Idle;
    });
    overlay::poll(&view, &program, cx);
    assert!(
        cx.debug_bounds("chat-loading").is_some(),
        "awaiting-fit alone should still show the loading shell"
    );
}

//! While an observed agent's first transcript is on its way, the real
//! terminal keeps its editor and keyboard path.

use super::*;
use crate::{
    SheprdWindow,
    chat::{
        existing::{Binding, Existing},
        link::Link,
        terminal::InputState,
    },
    sidebar::layout_tests::full_draw,
    window::MockPeer,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use sheprd_chat_protocol::ByeReason;
use sheprd_client::{
    ConnectTarget,
    protocol::{AgentStatus, ClientPaneInputEvent},
};

fn target() -> Target {
    Target {
        endpoint: "0123456789abcdef0123456789abcdef".parse().unwrap(),
        existing: Some(Binding {
            pane_id: "w1:p1".into(),
            agent: sheprd_chat_protocol::Agent::Codex,
            session_id: "12345678-1234-4123-8123-123456789abc".into(),
            herdr_session: None,
            pid: 21,
            shell_pid: 20,
            started_ticks: 123,
        }),
        pane_id: "w1:p1".into(),
        route: Route::Local,
        surface: Surface::Herdr,
    }
}

/// Attaches the observed target to `program` and draws. The window's own poll
/// would ask a real host for bindings, so this goes straight to its chats.
fn poll(view: &Entity<SheprdWindow>, program: &Program, cx: &mut VisualTestContext) {
    let path = program.path.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            let now = Instant::now();
            view.chats
                .reconcile(vec![target()], now, &mut |_, since, resend| {
                    Link::start(path.clone(), since, resend)
                });
            view.chats.poll(now);
        });
        full_draw(window, cx).clear(cx);
    });
}

fn set_status(view: &Entity<SheprdWindow>, status: AgentStatus, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status = status;
    });
}

/// A window whose fixture pane is idle, focused, and sized as it asked.
fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, MockPeer, &mut VisualTestContext) {
    let (view, peer, cx) = overlay::open(cx, ConnectTarget::Local);
    cx.simulate_resize(size(px(1100.), px(900.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    selection::acknowledge_size(&view, cx);
    set_status(&view, AgentStatus::Idle, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            Arc::make_mut(view.live.surface.as_mut().unwrap())
                .frame
                .cursor = None;
            window.focus(&view.focus, cx);
        });
    });
    (view, peer, cx)
}

fn pending(
    cx: &mut TestAppContext,
) -> (
    Entity<SheprdWindow>,
    MockPeer,
    Program,
    Peer,
    &mut VisualTestContext,
) {
    let (view, peer, cx) = window(cx);
    let program = Program::new();
    poll(&view, &program, cx);
    let (mut stream, _) = program.accept();
    send(&mut stream, &welcome());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_some());
    assert!(cx.debug_bounds("chat-view").is_none());
    assert!(!view.read_with(cx, |view, _| view.chats.shown("w1:p1")));
    (view, peer, program, stream, cx)
}

#[gpui::test]
fn a_delayed_snapshot_keeps_terminal_input_and_then_replaces_the_loading_view(
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
        poll(&view, &program, cx);
        cx.debug_bounds("chat-view").is_some()
    });
    assert!(cx.debug_bounds("chat-loading").is_none());
}

#[gpui::test]
fn use_terminal_persists_when_the_first_snapshot_arrives(cx: &mut TestAppContext) {
    let (view, _peer, program, mut stream, cx) = pending(cx);
    let terminal = cx.debug_bounds("chat-loading-terminal").unwrap();
    cx.simulate_click(terminal.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_none());
    assert!(view.read_with(cx, |view, _| view.chats.raw("w1:p1")));
    send(
        &mut stream,
        &snapshot(cursor(1, 0), vec![agent(1, "Ready.")]),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-switch").is_some()
    });
    assert!(cx.debug_bounds("chat-view").is_none());
}

#[gpui::test]
fn use_terminal_during_discovery_carries_over_to_the_chat_it_finds(cx: &mut TestAppContext) {
    let (view, _peer, cx) = window(cx);
    view.update(cx, |view, _| {
        view.chats.existing = Existing::pending("w1:p1");
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let terminal = cx.debug_bounds("chat-loading-terminal").unwrap();
    cx.simulate_click(terminal.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("chat-loading").is_none());
    let program = Program::new();
    poll(&view, &program, cx);
    let (_stream, _) = program.accept();
    assert!(view.read_with(cx, |view, _| view.chats.raw("w1:p1")));
    assert!(cx.debug_bounds("chat-loading").is_none());
}

#[gpui::test]
fn blocked_or_failed_attachments_reveal_the_original_terminal(cx: &mut TestAppContext) {
    let (view, _peer, program, mut stream, cx) = pending(cx);
    set_status(&view, AgentStatus::Blocked, cx);
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_none());
    set_status(&view, AgentStatus::Idle, cx);
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-loading").is_some());
    send(
        &mut stream,
        &ServerMessage::Bye {
            reason: ByeReason::Exiting,
        },
    );
    wait_until(|| {
        poll(&view, &program, cx);
        !view.read_with(cx, |view, _| view.chats.pane("w1:p1").unwrap().has_link())
    });
    assert!(cx.debug_bounds("chat-loading").is_none());
    assert!(cx.debug_bounds("chat-view").is_none());
}

#[gpui::test]
fn a_blocked_editor_stays_uncovered_even_when_its_geometry_awaits_fit(cx: &mut TestAppContext) {
    let (view, _peer, program, _stream, cx) = pending(cx);
    set_status(&view, AgentStatus::Blocked, cx);
    view.update(cx, |view, _| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.frame.height = view.options.surface_size.rows + 8;
        surface.panes[0].rect.height = surface.frame.height;
        surface.panes[0].inner_rect.height = surface.frame.height;
        let surface = view.live.surface.as_ref().unwrap();
        let layout = view.existing_terminal_layout(&surface.panes[0], surface, Default::default());
        assert_eq!(layout.state, InputState::AwaitingFit);
        assert!(layout.conversation.height > 0);
    });
    poll(&view, &program, cx);
    assert!(view.read_with(cx, |view, _| view.chats.pane("w1:p1").unwrap().loading()));
    assert!(cx.debug_bounds("chat-loading").is_none());
    set_status(&view, AgentStatus::Idle, cx);
    poll(&view, &program, cx);
    assert!(
        cx.debug_bounds("chat-loading").is_some(),
        "awaiting-fit alone should still show the loading view"
    );
}

#[gpui::test]
fn a_chat_run_by_its_own_program_keeps_its_terminal_while_it_connects(cx: &mut TestAppContext) {
    let (view, _peer, cx) = window(cx);
    let program = Program::new();
    overlay::poll(&view, &program, cx);
    let (_stream, _) = program.accept();
    overlay::poll(&view, &program, cx);
    assert!(!view.read_with(cx, |view, _| view.chats.pane("w1:p1").unwrap().loading()));
    assert!(cx.debug_bounds("chat-loading").is_none());
}

//! Rich output and the existing CLI's own editor share one pane. Synthetic
//! daemon peers verify keyboard input reaches that original terminal exactly
//! once and never becomes a command to the read-only transcript observer.

use super::*;
use crate::{
    HerdrWindow,
    chat::{
        existing::Binding,
        terminal::{InputSize, InputState},
    },
    sidebar::layout_tests::full_draw,
    window::MockPeer,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use herdr_client::{
    ConnectTarget,
    protocol::{AgentStatus, ClientMessage as HerdrMessage, ClientPaneInputEvent},
};

pub(super) fn observed(
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
    let mut transcript = snapshot(
        cursor(1, 0),
        vec![agent(1, "The same running conversation.")],
    );
    if let ServerMessage::Snapshot { transcript, .. } = &mut transcript {
        transcript.read_only = true;
    }
    let stream = overlay::attach_with(&view, &program, transcript, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let pane = view.chats.pane_mut("w1:p1").unwrap();
            pane.target.existing = Some(Binding {
                pane_id: "w1:p1".into(),
                agent: herdr_chat_protocol::Agent::Codex,
                session_id: "synthetic-session".into(),
                herdr_session: Some("synthetic-daemon".into()),
                pid: 123,
                shell_pid: 100,
                started_ticks: 567,
            });
            pane.read_only_focus = None;
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                AgentStatus::Idle;
            Arc::make_mut(view.live.surface.as_mut().unwrap())
                .frame
                .cursor = None;
            window.focus(&view.focus, cx);
        });
        full_draw(window, cx).clear(cx);
        // The mock daemon has no resize loop. Answer the dimensions requested
        // by the real window before testing its native terminal input path.
        view.update(cx, |view, _| {
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.frame.width = view.options.surface_size.cols;
            surface.frame.height = view.options.surface_size.rows;
            let rect = herdr_client::protocol::SurfaceRect {
                x: 0,
                y: 0,
                width: surface.frame.width,
                height: surface.frame.height,
            };
            surface.panes[0].rect = rect;
            surface.panes[0].inner_rect = rect;
        });
        full_draw(window, cx).clear(cx);
        view.read_with(cx, |view, _| assert!(view.input_ready()));
    });
    (view, peer, program, stream, cx)
}

pub(super) fn next_input(peer: &mut MockPeer) -> Vec<ClientPaneInputEvent> {
    loop {
        if let HerdrMessage::ClientShellPaneInput { pane_id, events } = peer.receive() {
            assert_eq!(pane_id, "w1:p1");
            return events;
        }
    }
}

#[gpui::test]
fn existing_agent_input_stays_in_the_rich_pane_and_uses_its_original_terminal(
    cx: &mut TestAppContext,
) {
    let (view, mut peer, _program, _stream, cx) = observed(cx);
    let chat = cx.debug_bounds("chat-view").unwrap();
    let bar = cx.debug_bounds("chat-terminal-input").unwrap();
    assert!(chat.bottom() <= bar.top());
    assert!(cx.debug_bounds("chat-reply-terminal").is_none());
    assert!(cx.debug_bounds("chat-prompt").is_none());
    // Reading prose keeps typing directed at the real editor below it.
    cx.simulate_click(chat.center(), Modifiers::default());
    assert!(cx.update(|window, cx| {
        let view = view.read(cx);
        view.focus.is_focused(window) && !view.chat_focused(window, cx)
    }));
    cx.simulate_input("continue here");
    // GPUI dispatches one commit per character on some platforms. Preserve
    // and check the exact native input stream rather than its batching.
    let mut typed = String::new();
    while typed.len() < "continue here".len() {
        for event in next_input(&mut peer) {
            let ClientPaneInputEvent::TextCommit(text) = event else {
                panic!("unexpected input before Enter: {event:?}");
            };
            typed.push_str(&text);
        }
    }
    assert_eq!(typed, "continue here");
    cx.simulate_keystrokes("enter");
    assert!(matches!(
        next_input(&mut peer).as_slice(),
        [ClientPaneInputEvent::Key {
            code: herdr_client::protocol::ClientKeyCode::Enter,
            ..
        }]
    ));
    // A following character must be next: no duplicate submit was inserted.
    cx.simulate_input("!");
    assert_eq!(
        next_input(&mut peer),
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
        assert!(!view.chats.raw("w1:p1"));
    });
}

#[gpui::test]
fn explicit_compact_overrides_a_blocked_prompts_automatic_expansion(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    let original = cx.debug_bounds("chat-terminal-input").unwrap();
    let button = cx.debug_bounds("chat-expand-input").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let expanded = cx.debug_bounds("chat-terminal-input").unwrap();
    assert!(expanded.top() < original.top());
    assert!(cx.debug_bounds("chat-view").is_some());
    assert!(!view.read_with(cx, |view, _| view.chats.raw("w1:p1")));
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.chats.pane_mut("w1:p1").unwrap().terminal_size = InputSize::Auto;
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                AgentStatus::Blocked;
        });
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(
        cx.debug_bounds("chat-terminal-input").unwrap().top(),
        expanded.top()
    );
    let button = cx.debug_bounds("chat-expand-input").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("chat-terminal-input").unwrap().top(),
        original.top()
    );
    view.read_with(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        assert_eq!(pane.terminal_size, InputSize::Compact);
        let surface = view.live.surface.as_ref().unwrap();
        let layout = view
            .chat_terminal_layout(&surface.panes[0], surface)
            .unwrap();
        assert_eq!(layout.state, InputState::Blocked);
        assert_eq!(layout.input.height, 8);
        assert!(!view.chats.raw("w1:p1"));
    });
}

#[gpui::test]
fn automatic_input_returns_compact_after_answer_but_manual_expansion_persists(
    cx: &mut TestAppContext,
) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    let original = cx.debug_bounds("chat-terminal-input").unwrap();
    for status in [
        AgentStatus::Blocked,
        AgentStatus::Working,
        AgentStatus::Blocked,
        AgentStatus::Idle,
    ] {
        cx.update(|window, cx| {
            view.update(cx, |view, _| {
                Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status = status;
            });
            full_draw(window, cx).clear(cx);
        });
        let input = cx.debug_bounds("chat-terminal-input").unwrap();
        if status == AgentStatus::Blocked {
            assert!(input.top() < original.top());
        } else {
            assert_eq!(input.top(), original.top());
        }
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.chats.pane("w1:p1").unwrap().terminal_size,
                InputSize::Auto
            )
        });
    }
    let button = cx.debug_bounds("chat-expand-input").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                AgentStatus::Done;
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("chat-terminal-input").unwrap().top() < original.top());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Expanded
        )
    });
}

#[gpui::test]
fn compact_blocked_input_keeps_a_high_editing_cursor_visible(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.chats.pane_mut("w1:p1").unwrap().terminal_size = InputSize::Compact;
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                AgentStatus::Blocked;
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            let rect = surface.panes[0].inner_rect;
            surface.panes[0].focused = true;
            surface.frame.cursor = Some(herdr_client::protocol::CursorState {
                x: rect.x + 1,
                y: rect.y + 4,
                visible: true,
                shape: Default::default(),
            });
        });
        full_draw(window, cx).clear(cx);
    });
    view.read_with(cx, |view, _| {
        let surface = view.live.surface.as_ref().unwrap();
        let layout = view
            .chat_terminal_layout(&surface.panes[0], surface)
            .unwrap();
        assert!(layout.input.y <= surface.frame.cursor.as_ref().unwrap().y);
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Compact
        );
    });
    let button = cx.debug_bounds("chat-expand-input").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        let surface = view.live.surface.as_ref().unwrap();
        let caret = surface.frame.cursor.as_ref().unwrap();
        let layout = view
            .chat_terminal_layout(&surface.panes[0], surface)
            .unwrap();
        assert_eq!(layout.state, InputState::Blocked);
        assert!(layout.input.height > 8);
        assert!(layout.input.y <= caret.y);
        assert!(caret.y < layout.input.y + layout.input.height);
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Expanded
        );
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let button = cx.debug_bounds("chat-expand-input").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Compact
        );
    });
}

#[gpui::test]
fn terminal_popups_still_cover_rich_output_for_every_input_size(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    view.update(cx, |view, cx| {
        let mut surface = view.live.surface.as_ref().unwrap().as_ref().clone();
        surface.popup = Some(Box::new(herdr_client::protocol::ClientShellPopupSurface {
            terminal_id: "synthetic-popup".into(),
            title: "Question".into(),
            width: None,
            height: None,
            frame: surface.frame.clone(),
            mouse_reporting: true,
            sgr_pixel_mouse: false,
            pixel_width: 640,
            pixel_height: 320,
        }));
        for size in [InputSize::Auto, InputSize::Compact, InputSize::Expanded] {
            view.chats.pane_mut("w1:p1").unwrap().terminal_size = size;
            assert!(view.render_chats(Some(&surface), cx).is_empty());
        }
    });
}

#[gpui::test]
fn working_output_cursor_does_not_hide_the_rich_conversation(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    let original = cx.debug_bounds("chat-view").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                AgentStatus::Working;
            Arc::make_mut(view.live.surface.as_mut().unwrap())
                .frame
                .cursor = Some(herdr_client::protocol::CursorState {
                x: 1,
                y: 1,
                visible: true,
                shape: Default::default(),
            });
        });
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(cx.debug_bounds("chat-view").unwrap(), original);
    assert!(cx.debug_bounds("chat-terminal-input").is_some());
    assert!(cx.debug_bounds("chat-cancel").is_none());
}

#[gpui::test]
fn oversized_shared_terminal_shows_a_fit_action_until_its_original_editor_fits(
    cx: &mut TestAppContext,
) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    let fitted = view.read_with(cx, |view, _| view.live.surface.clone().unwrap());
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.last_input = None;
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.frame.width += 50;
            surface.frame.height += 12;
            surface.panes[0].rect.width = surface.frame.width;
            surface.panes[0].rect.height = surface.frame.height;
            surface.panes[0].inner_rect = surface.panes[0].rect;
        });
        full_draw(window, cx).clear(cx);
    });
    view.read_with(cx, |view, _| {
        let surface = view.live.surface.as_ref().unwrap();
        let layout = view
            .chat_terminal_layout(&surface.panes[0], surface)
            .unwrap();
        assert_eq!(layout.state, InputState::AwaitingFit);
        assert_eq!(layout.input.height, 0);
        assert!(!view.input_ready());
        assert!(
            view.last_input.is_none(),
            "rendering must not reclaim another client's size"
        );
    });
    let fit = cx.debug_bounds("chat-fit-input").unwrap();
    let terminal = view.read_with(cx, |view, _| view.bounds);
    assert!(fit.right() <= terminal.right());
    assert!(fit.bottom() <= terminal.bottom());
    assert!(cx.debug_bounds("chat-expand-input").is_none());
    cx.simulate_click(fit.center(), Modifiers::default());
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.focus.is_focused(window));
        assert!(
            view.last_input.is_some(),
            "the explicit click allows normal geometry reassert"
        );
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Auto
        );
    });
    // The daemon's ordinary geometry response makes the exact editor visible
    // again; no rescaled cells, translated input, or synthetic prompt is used.
    cx.update(|window, cx| {
        view.update(cx, |view, _| view.live.surface = Some(fitted));
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("chat-fit-input").is_none());
    assert!(cx.debug_bounds("chat-expand-input").is_some());
    view.update(cx, |view, _| {
        assert!(view.input_ready());
        assert!(
            view.chats
                .pane_mut("w1:p1")
                .unwrap()
                .session
                .resend()
                .is_empty()
        );
    });
}

#[gpui::test]
fn existing_agent_file_drop_uses_terminal_paste_without_submitting(cx: &mut TestAppContext) {
    let (view, mut peer, _program, _stream, cx) = observed(cx);
    let bar = cx.debug_bounds("chat-terminal-input").unwrap();
    cx.simulate_click(bar.center(), Modifiers::default());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.drop_terminal_files(
                &gpui::ExternalPaths(vec!["/tmp/notes.txt".into()].into()),
                window,
                cx,
            )
        });
    });
    let events = next_input(&mut peer);
    assert!(
        matches!(events.as_slice(), [ClientPaneInputEvent::Paste(text)] if text.contains("/tmp/notes.txt"))
    );
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert!(pane.attachments.is_empty());
        assert!(pane.session.resend().is_empty());
    });
}

mod resize;

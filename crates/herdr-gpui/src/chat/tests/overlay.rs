//! The chat view in a headless window: drawn over its pane only for a local
//! daemon, typed into instead of the terminal, and turned back to the
//! terminal and again.

use super::*;
use crate::{
    HerdrWindow,
    chat::{link::Link, view::pane_bounds},
    sidebar::layout_tests::{fixture_window, full_draw},
    window::MockPeer,
};
use gpui::{ClipboardItem, Entity, ExternalPaths, Modifiers, TestAppContext, VisualTestContext};
use herdr_chat_protocol::Command;
use herdr_client::{
    ConnectTarget,
    protocol::{ClientMessage as HerdrMessage, ClientPaneInputEvent},
};
use std::sync::Arc;

const ENDPOINT: &str = "0123456789abcdef0123456789abcdef";

pub(super) fn open(
    cx: &mut TestAppContext,
    target: ConnectTarget,
) -> (Entity<HerdrWindow>, MockPeer, &mut VisualTestContext) {
    let peer = MockPeer::advertising(&[]);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.features.acp_chats = true;
        peer.prepare(&mut view, cx);
        view.endpoints[0].connection.target = target;
        view.live.local_daemon_peer = true;
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        // Notification overlays have their own tests and would intercept chat
        // pointer events in this fixture, especially in a narrow window.
        snapshot.product_announcement = None;
        snapshot.config_diagnostic = None;
        snapshot.agents[0].tokens = vec![(herdr_chat_protocol::TOKEN.into(), ENDPOINT.into())];
        window.focus(&view.focus, cx);
        view
    });
    (view, peer, cx)
}

/// Polls the window's chats, connecting them to `program`.
pub(super) fn poll(view: &Entity<HerdrWindow>, program: &Program, cx: &mut VisualTestContext) {
    let path = program.path.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.poll_chats_with(
                &mut |_, since, resend| Link::start(path.clone(), since, resend),
                window,
                cx,
            );
        });
        full_draw(window, cx).clear(cx);
    });
}

fn attach(view: &Entity<HerdrWindow>, program: &Program, cx: &mut VisualTestContext) -> Peer {
    attach_with(
        view,
        program,
        snapshot(cursor(1, 0), vec![agent(1, "Ready.")]),
        cx,
    )
}

/// Attaches the chat over the fixture's pane to `program`, which greets it
/// with `snapshot`.
pub(super) fn attach_with(
    view: &Entity<HerdrWindow>,
    program: &Program,
    snapshot: ServerMessage,
    cx: &mut VisualTestContext,
) -> Peer {
    poll(view, program, cx);
    let (mut stream, _) = program.accept();
    send(&mut stream, &welcome());
    send(&mut stream, &snapshot);
    wait_until(|| {
        poll(view, program, cx);
        cx.debug_bounds("chat-view").is_some()
    });
    stream
}

#[gpui::test]
fn a_local_chat_pane_is_drawn_over_its_cells(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach(&view, &program, cx);
    let chat = cx.debug_bounds("chat-view").unwrap();
    let terminal = view.read_with(cx, |view, _| view.bounds);
    let pane = pane_bounds(
        herdr_client::protocol::SurfaceRect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        },
        view.read_with(cx, |view, _| view.cell_width),
        view.read_with(cx, |view, _| view.config.terminal.line_height()),
    );
    assert_eq!(chat.origin, terminal.origin + pane.origin);
    assert_eq!(chat.size, pane.size);
    assert!(cx.debug_bounds("chat-agent").is_some());
}

#[gpui::test]
fn an_ssh_daemons_chat_is_dialled_over_ssh_and_keeps_its_terminal_until_it_answers(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(
        cx,
        ConnectTarget::Ssh {
            target: "host".into(),
            session: "default".into(),
        },
    );
    let mut routes = Vec::new();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.poll_chats_with(
                &mut |target, _, _| {
                    routes.push(target.route.clone());
                    Err(Error::Closed)
                },
                window,
                cx,
            );
        });
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(routes, [Route::Ssh("host".into())]);
    assert!(cx.debug_bounds("chat-view").is_none());
}

#[gpui::test]
fn typing_goes_to_the_chat_not_the_terminal(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach(&view, &program, cx);
    assert!(cx.update(|window, cx| view.read(cx).chat_focused(window, cx)));
    cx.simulate_input("first");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("second");
    cx.simulate_keystrokes("enter");
    let Some(ClientMessage::Command { command, .. }) = read(&mut stream) else {
        panic!("a command");
    };
    assert_eq!(
        command,
        Command::Prompt {
            text: "first\nsecond".into(),
            attachments: Vec::new(),
        }
    );
    // Nothing reached the pane: the next pane input is the sentinel.
    view.read_with(cx, |view, _| {
        crate::connection::ConnectionBridge::send_input(
            view.endpoints[0].connection.handle.as_ref().unwrap(),
            &view.live.snapshot.as_ref().unwrap().boot_id,
            &crate::terminal::InputTarget::Pane("w1:p1".into()),
            ClientPaneInputEvent::TextCommit("sentinel".into()),
        )
        .unwrap();
    });
    loop {
        if let HerdrMessage::ClientShellPaneInput { events, .. } = peer.receive() {
            assert_eq!(
                events,
                [ClientPaneInputEvent::TextCommit("sentinel".into())]
            );
            break;
        }
    }
}

#[gpui::test]
fn the_terminal_can_be_shown_instead_and_back(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach(&view, &program, cx);
    let button = cx.debug_bounds("chat-terminal").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-view").is_none());
    assert!(!cx.update(|window, cx| view.read(cx).chat_focused(window, cx)));
    let switch = cx.debug_bounds("chat-switch").unwrap();
    cx.simulate_click(switch.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-view").is_some());
}

#[gpui::test]
fn a_read_only_overlay_blocks_keys_paste_and_file_drops_until_switched_back(
    cx: &mut TestAppContext,
) {
    let (view, mut peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = snapshot(cursor(1, 0), vec![agent(1, "Live transcript")]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut transcript {
        transcript.read_only = true;
    }
    let _stream = attach_with(&view, &program, transcript, cx);
    assert!(cx.debug_bounds("chat-read-only").is_some());
    assert!(cx.update(|window, cx| {
        let view = view.read(cx);
        view.chat_focused(window, cx) && !view.focus.is_focused(window)
    }));
    let chat = cx.debug_bounds("chat-view").unwrap();
    cx.simulate_click(chat.center(), Modifiers::default());
    cx.simulate_input("do not send");
    cx.simulate_keystrokes("enter ctrl-c");
    cx.write_to_clipboard(ClipboardItem::new_string("do not paste\n".into()));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    });
    cx.dispatch_action(crate::actions::Paste);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.drop_terminal_files(
                &ExternalPaths(vec!["/work/notes.md".into()].into()),
                window,
                cx,
            );
        });
    });
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert!(pane.session.resend().is_empty());
        assert!(pane.attachments.is_empty());
        assert_eq!(pane.uploading, 0);
    });

    let target = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().target.clone()
    });
    let button = cx.debug_bounds("chat-reply-terminal").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-view").is_none());
    assert!(cx.update(|window, cx| {
        let view = view.read(cx);
        !view.chat_focused(window, cx) && view.focus.is_focused(window)
    }));
    view.read_with(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        assert_eq!(pane.target, target);
        assert_eq!(
            pane.session.transcript().entries,
            [agent(1, "Live transcript")]
        );
    });

    // A real daemon peer proves no key, text, or clipboard event preceded
    // this sentinel, including the reply button and paths that bypass
    // Session::submit entirely. A terminal draft therefore stays untouched.
    view.read_with(cx, |view, _| {
        crate::connection::ConnectionBridge::send_input(
            view.endpoints[0].connection.handle.as_ref().unwrap(),
            &view.live.snapshot.as_ref().unwrap().boot_id,
            &crate::terminal::InputTarget::Pane("w1:p1".into()),
            ClientPaneInputEvent::TextCommit("sentinel".into()),
        )
        .unwrap();
    });
    loop {
        if let HerdrMessage::ClientShellPaneInput { events, .. } = peer.receive() {
            assert_eq!(
                events,
                [ClientPaneInputEvent::TextCommit("sentinel".into())]
            );
            break;
        }
    }

    let switch = cx.debug_bounds("chat-switch").unwrap();
    cx.simulate_click(switch.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.update(|window, cx| view.read(cx).chat_focused(window, cx)));
}

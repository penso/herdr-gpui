//! Chats in herdr-core panes, attached in a headless window: a mirror tab
//! shows none, a click moves the pane in use without a Herdr connection,
//! and the CLI's default pane is the herdr-core chat holding the keyboard.

use super::{overlay::open, *};
use crate::{
    HerdrWindow, browser::TabId, chat::core_pane::key, core_terminals::panes::Panes,
    sidebar::layout_tests::full_draw,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use herdr_backend::{RunState, TerminalId, TerminalInfo};
use herdr_client::{ConnectTarget, protocol::AgentStatus};
use std::collections::BTreeMap;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

fn info(name: &str, endpoint: Option<&str>) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: None,
        id: id(name),
        labels: endpoint
            .map(|endpoint| {
                BTreeMap::from([(herdr_chat_protocol::TOKEN.to_owned(), endpoint.to_owned())])
            })
            .unwrap_or_default(),
        name: None,
        state: RunState::Running,
        title: None,
    }
}

const DRIVEN: u64 = 1;
const MIRROR: u64 = 2;

/// A window whose only chat runs in herdr-core terminal t1, split beside a
/// shell in t2 that is in use, with a mirror tab of t1; the chat attached to
/// `program`.
fn attached(
    cx: &mut TestAppContext,
) -> (Entity<HerdrWindow>, Peer, Program, &mut VisualTestContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let endpoint = "fedcba9876543210fedcba9876543210";
    view.update(cx, |view, _| {
        // The fixture's Herdr pane runs no chat here.
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.agents[0].tokens.clear();
        let mut panes = Panes::single(id("t1"));
        panes
            .split(&id("t1"), crate::core_terminals::panes::Axis::Row, id("t2"))
            .unwrap();
        view.core_terminals.show_for_test(
            TabId::test(DRIVEN),
            panes,
            false,
            vec![info("t1", Some(endpoint)), info("t2", None)],
        );
        view.core_terminals
            .set_active(TabId::test(DRIVEN), id("t2"));
        view.core_terminals.show_for_test(
            TabId::test(MIRROR),
            Panes::single(id("t1")),
            true,
            Vec::new(),
        );
    });
    let path = program.path.clone();
    let poll = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.poll_chats_with(
                    &mut |_, since, resend| {
                        crate::chat::link::Link::start(path.clone(), since, resend)
                    },
                    window,
                    cx,
                );
            });
            full_draw(window, cx).clear(cx);
        });
    };
    poll(cx);
    let (mut stream, _) = program.accept();
    send(&mut stream, &welcome());
    send(
        &mut stream,
        &snapshot(cursor(1, 0), vec![agent(1, "Ready.")]),
    );
    wait_until(|| {
        poll(cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane(&key(&id("t1")))
                .is_some_and(|chat| chat.available() && chat.composer.is_some())
        })
    });
    (view, stream, program, cx)
}

#[gpui::test]
fn only_the_tab_that_drives_the_terminal_gives_its_chat_the_keyboard(cx: &mut TestAppContext) {
    let (view, _stream, _program, cx) = attached(cx);
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.chats
                .pane(&key(&id("t1")))
                .map(|chat| &chat.target.surface),
            Some(&Surface::Core(id("t1")))
        );
        assert!(
            view.core_chat_focus(TabId::test(DRIVEN), &id("t1"), window, cx)
                .is_some()
        );
        // A mirror only watches: no composer, no permission buttons.
        assert!(
            view.core_chat_focus(TabId::test(MIRROR), &id("t1"), window, cx)
                .is_none()
        );
        assert!(view.core_terminals.drives(TabId::test(DRIVEN)));
        assert!(!view.core_terminals.drives(TabId::test(MIRROR)));
    });
}

#[gpui::test]
fn a_click_on_a_core_chat_moves_the_pane_in_use_without_herdr(cx: &mut TestAppContext) {
    let (view, _stream, _program, cx) = attached(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // No Herdr frame is ready for input.
            view.live.surface = None;
            assert!(!view.input_ready());
            assert_eq!(
                view.core_terminals.chat_split_at(TabId::test(DRIVEN)),
                Some(id("t2"))
            );
            // What a click on the chat runs.
            view.focus_chat(&key(&id("t1")), window, cx);
            assert_eq!(
                view.core_terminals.chat_split_at(TabId::test(DRIVEN)),
                Some(id("t1"))
            );
        });
    });
}

#[gpui::test]
fn the_default_pane_is_the_core_chat_holding_the_keyboard(cx: &mut TestAppContext) {
    let (view, _stream, _program, cx) = attached(cx);
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).focused_core_chat_key(window, cx), None);
        let focus = view
            .read(cx)
            .core_chat_focus(TabId::test(DRIVEN), &id("t1"), window, cx)
            .unwrap();
        window.focus(&focus, cx);
        assert_eq!(
            view.read(cx).focused_core_chat_key(window, cx).as_deref(),
            Some("core:t1")
        );
    });
}

//! The prompt pinned over the conversation: there once it scrolls away, in
//! every layout, and one click from the prompt itself.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{SheprdWindow, config::ChatLayoutMode};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use sheprd_client::ConnectTarget;

fn redraw(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

/// A prompt, then a reply too long for the window.
fn opened(view: &Entity<SheprdWindow>, program: &Program, cx: &mut VisualTestContext) -> Peer {
    let prompt = Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: "Why does the sidebar flicker?".into(),
            files: 0,
        },
    };
    let entries = std::iter::once(prompt)
        .chain((2..=40).map(|id| agent(id, "A line of the reply.")))
        .collect();
    let stream = attach_with(view, program, snapshot(cursor(1, 0), entries), cx);
    redraw(view, cx);
    stream
}

#[gpui::test]
fn every_layout_pins_the_prompt_it_scrolled_past(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, cx);
    for &mode in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(mode, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        redraw(&view, cx);
        assert!(cx.debug_bounds("chat-pinned").is_some(), "{mode:?}");
        assert!(cx.debug_bounds("chat-pinned-text").is_some(), "{mode:?}");
    }
}

#[gpui::test]
fn the_pin_sits_on_top_of_the_list(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, cx);
    let pin = cx.debug_bounds("chat-pinned").unwrap();
    let list = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.viewport_bounds()
    });
    assert!(pin.top() <= list.top());
    assert_eq!(pin.size.width, list.size.width);
}

#[gpui::test]
fn clicking_the_pin_goes_to_the_prompt_and_the_pin_leaves(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, cx);

    let pin = cx.debug_bounds("chat-pinned").unwrap();
    cx.simulate_click(pin.center(), Modifiers::default());
    redraw(&view, cx);

    let top = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.logical_scroll_top()
    });
    assert_eq!(top.item_ix, 0);
    assert!(cx.debug_bounds("chat-pinned").is_none());
}

#[gpui::test]
fn a_short_chat_shows_no_pin(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, timeline::conversation(false), cx);
    redraw(&view, cx);
    assert!(cx.debug_bounds("chat-pinned").is_none());
}

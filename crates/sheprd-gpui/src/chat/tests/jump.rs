//! The button back to the end: there while the list is left above its end,
//! gone at the end and while a glide is on its way there, and one click
//! from the end and following again.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{SheprdWindow, window::MockPeer};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use sheprd_client::ConnectTarget;

fn behind(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> f32 {
    view.read_with(cx, |view, _| {
        let list = &view.chats.pane("w1:p1").unwrap().list;
        f32::from(list.max_offset_for_scrollbar().y + list.scroll_px_offset_for_scrollbar().y)
    })
}

fn redraw(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn away(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        let glide = &view.chats.pane("w1:p1").unwrap().glide;
        glide
            .away_shown(crate::motion::now() + crate::motion::LEAVE * 2)
            .is_some()
    })
}

fn place(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> gpui::Pixels {
    view.read_with(cx, |view, _| {
        let list = &view.chats.pane("w1:p1").unwrap().list;
        list.scroll_px_offset_for_scrollbar().y
    })
}

fn wheel(delta: f32, cx: &mut VisualTestContext) {
    let chat = cx.debug_bounds("chat-view").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: chat.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(delta))),
        ..Default::default()
    });
}

/// Opens 40 entries, drawn at the end of the list.
fn opened(view: &Entity<SheprdWindow>, program: &Program, cx: &mut VisualTestContext) -> Peer {
    let entries = (1..=40)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    let stream = attach_with(view, program, snapshot(cursor(1, 0), entries), cx);
    redraw(view, cx);
    stream
}

/// Grows the last entry by a few paragraphs and waits for it to arrive.
fn grow_last(
    view: &Entity<SheprdWindow>,
    program: &Program,
    stream: &mut Peer,
    cx: &mut VisualTestContext,
) {
    send(
        stream,
        &change(
            append(40, &"\n\nMore of the reply.".repeat(8)),
            cursor(1, 1),
        ),
    );
    wait_until(|| {
        poll(view, program, cx);
        view.read_with(cx, |view, _| {
            let pane = view.chats.pane("w1:p1").unwrap();
            let entries = &pane.session.transcript().entries;
            matches!(&entries[39].kind, EntryKind::Agent { text } if text.len() > 40)
        })
    });
    redraw(view, cx);
}

fn gliding(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, MockPeer, &mut VisualTestContext) {
    let (view, peer, cx) = open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| view.config.chat.glide = true);
    (view, peer, cx)
}

#[gpui::test]
fn the_button_is_gone_at_the_end(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, cx);
    assert!(behind(&view, cx) <= 1.);
    assert!(cx.debug_bounds("chat-jump").is_none());
}

#[gpui::test]
fn scrolling_up_shows_the_button_and_scrolling_back_closes_it(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, cx);

    wheel(200., cx);
    redraw(&view, cx);
    assert!(
        cx.debug_bounds("chat-jump").is_some(),
        "shown when scrolled up"
    );

    wheel(-400., cx);
    redraw(&view, cx);
    assert!(behind(&view, cx) <= 1.);
    assert!(!away(&view, cx), "closes back at the end");
}

#[gpui::test]
fn a_click_goes_to_the_end_and_follows_again(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    wheel(200., cx);
    redraw(&view, cx);
    let before = behind(&view, cx);
    assert!(before > 100., "scrolled up: {before}");

    let button = cx.debug_bounds("chat-jump").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    redraw(&view, cx);
    assert!(behind(&view, cx) <= 1., "one click reaches the end");
    assert!(!away(&view, cx));

    grow_last(&view, &program, &mut stream, cx);
    assert!(behind(&view, cx) <= 1., "new output is followed again");
}

#[gpui::test]
fn a_glide_on_its_way_to_the_end_shows_no_button(cx: &mut TestAppContext) {
    let (view, _peer, cx) = gliding(cx);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    grow_last(&view, &program, &mut stream, cx);
    assert!(behind(&view, cx) > 20., "still gliding");
    assert!(cx.debug_bounds("chat-jump").is_none());
}

#[gpui::test]
fn a_selection_holding_the_list_shows_the_button_that_still_reaches_the_end(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        pane.selection.select_all(pane.prepared.text());
    });
    // A new entry, as changing a selected one would drop the selection.
    let reply = "A new reply.\n\n".repeat(8);
    send(
        &mut stream,
        &change(
            Change::Push {
                entry: agent(41, &reply),
            },
            cursor(1, 1),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let pane = view.chats.pane("w1:p1").unwrap();
            pane.session.transcript().entries.len() == 41
        })
    });
    redraw(&view, cx);
    redraw(&view, cx);
    assert!(behind(&view, cx) > 20., "the selection holds the list");

    let button = cx.debug_bounds("chat-jump").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    redraw(&view, cx);
    assert!(behind(&view, cx) <= 1.);

    // Asking for the end lets go of the selection, so the list follows.
    let later = "A later reply.\n\n".repeat(8);
    send(
        &mut stream,
        &change(
            Change::Push {
                entry: agent(42, &later),
            },
            cursor(1, 2),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let pane = view.chats.pane("w1:p1").unwrap();
            pane.session.transcript().entries.len() == 42
        })
    });
    redraw(&view, cx);
    redraw(&view, cx);
    assert!(behind(&view, cx) <= 1., "following again");
    assert!(view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().selection.is_empty()
    }));
}

#[gpui::test]
fn a_chunk_landing_with_the_scroll_does_not_pull_the_list_back_down(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    // A reply taller than the scroll.
    grow_last(&view, &program, &mut stream, cx);
    assert!(behind(&view, cx) <= 1., "following the reply");

    // The user scrolls and a chunk lands before the next frame, with no
    // draw between them: the grown row is unmeasured until that frame.
    view.update(cx, |view, cx| {
        let pane = view.chats.pane("w1:p1").unwrap();
        pane.list.scroll_by(gpui::px(-60.));
        pane.glide.user_scrolled();
        pane.remeasure(EntryId(40));
        cx.notify();
    });
    cx.run_until_parked();
    redraw(&view, cx);
    let left = behind(&view, cx);
    assert!(left > 40., "the reader stays where they scrolled: {left}");
    assert!(cx.debug_bounds("chat-jump").is_some());
}

#[gpui::test]
fn output_below_a_reader_holds_their_place_and_says_it_is_waiting(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    wheel(200., cx);
    redraw(&view, cx);
    assert!(cx.debug_bounds("chat-jump").is_some());
    assert!(
        cx.debug_bounds("chat-jump-waiting").is_none(),
        "nothing new yet"
    );
    let top = place(&view, cx);

    grow_last(&view, &program, &mut stream, cx);
    redraw(&view, cx);
    assert_eq!(place(&view, cx), top, "the place holds");
    assert!(cx.debug_bounds("chat-jump-waiting").is_some());

    let button = cx.debug_bounds("chat-jump").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    redraw(&view, cx);
    assert!(behind(&view, cx) <= 1., "one click reaches the new output");
}

#[gpui::test]
fn a_snapshot_sent_again_keeps_a_reader_where_they_are(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    wheel(200., cx);
    redraw(&view, cx);
    let top = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.logical_scroll_top()
    });

    let entries = (1..=41)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    send(&mut stream, &snapshot(cursor(2, 0), entries));
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let pane = view.chats.pane("w1:p1").unwrap();
            pane.session.transcript().entries.len() == 41
        })
    });
    redraw(&view, cx);
    redraw(&view, cx);
    let after = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.logical_scroll_top()
    });
    assert_eq!(
        (after.item_ix, after.offset_in_item),
        (top.item_ix, top.offset_in_item)
    );
    assert!(cx.debug_bounds("chat-jump-waiting").is_some());
}

#[gpui::test]
fn a_reply_that_grew_across_a_reconnect_says_it_is_waiting(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    wheel(200., cx);
    redraw(&view, cx);
    assert!(cx.debug_bounds("chat-jump-waiting").is_none());

    // The same entries, with the last reply longer than it was.
    let entries = (1..=40)
        .map(|id| {
            let text = if id == 40 {
                "A line of an earlier reply, and what came after it."
            } else {
                "A line of an earlier reply."
            };
            agent(id, text)
        })
        .collect();
    send(&mut stream, &snapshot(cursor(2, 0), entries));
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let pane = view.chats.pane("w1:p1").unwrap();
            matches!(&pane.session.transcript().entries[39].kind,
                EntryKind::Agent { text } if text.contains("what came after"))
        })
    });
    redraw(&view, cx);
    redraw(&view, cx);
    assert!(cx.debug_bounds("chat-jump-waiting").is_some());
}

#[gpui::test]
fn the_same_transcript_sent_again_waits_for_nothing(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = opened(&view, &program, cx);
    wheel(200., cx);
    redraw(&view, cx);

    let before = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().session.snapshots()
    });
    let entries = (1..=40)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    send(&mut stream, &snapshot(cursor(2, 0), entries));
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().session.snapshots() > before
        })
    });
    redraw(&view, cx);
    redraw(&view, cx);
    assert!(
        cx.debug_bounds("chat-jump").is_some(),
        "still above the end"
    );
    assert!(cx.debug_bounds("chat-jump-waiting").is_none());
}

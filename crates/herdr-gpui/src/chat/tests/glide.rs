//! Following a streaming reply: the list opens at its end, and output that
//! grows the last entry shows at once, or with `[chat] glide` on glides
//! into view over several frames.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{HerdrWindow, window::MockPeer};
use gpui::{Entity, TestAppContext, VisualTestContext};
use herdr_client::ConnectTarget;

/// How far the list's top is from its end, in pixels.
fn behind(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> f32 {
    view.read_with(cx, |view, _| {
        let list = &view.chats.pane("w1:p1").unwrap().list;
        f32::from(list.max_offset_for_scrollbar().y + list.scroll_px_offset_for_scrollbar().y)
    })
}

/// A window whose chats glide to new output.
fn gliding(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, MockPeer, &mut VisualTestContext) {
    let (view, peer, cx) = open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| view.config.chat.glide = true);
    (view, peer, cx)
}

/// Opens 40 entries and grows the last by several paragraphs, then draws
/// one frame and says how far behind its end the list is.
fn grow_last(view: &Entity<HerdrWindow>, program: &Program, cx: &mut VisualTestContext) -> f32 {
    let entries = (1..=40)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    let mut stream = attach_with(view, program, snapshot(cursor(1, 0), entries), cx);
    redraw(view, cx);
    assert!(behind(view, cx) <= 1., "opens at its end");

    let more = "\n\nAnother paragraph of the reply as it streams in.".repeat(6);
    send(&mut stream, &change(append(40, &more), cursor(1, 1)));
    wait_until(|| {
        poll(view, program, cx);
        view.read_with(cx, |view, _| {
            let entries = &view
                .chats
                .pane("w1:p1")
                .unwrap()
                .session
                .transcript()
                .entries;
            matches!(&entries[39].kind, EntryKind::Agent { text } if text.len() > 100)
        })
    });
    redraw(view, cx);
    behind(view, cx)
}

fn redraw(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

#[gpui::test]
fn a_growing_reply_shows_at_once_by_default(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let behind = grow_last(&view, &program, cx);
    assert!(
        behind <= 1.,
        "the new lines show in the frame they land in: {behind}"
    );
}

#[gpui::test]
fn a_growing_reply_glides_into_view(cx: &mut TestAppContext) {
    let (view, _peer, cx) = gliding(cx);
    let program = Program::new();
    let start = grow_last(&view, &program, cx);
    assert!(
        start > 20.,
        "the new lines wait below rather than jump: {start}"
    );

    let mut frames = 0;
    let mut last = start;
    while behind(&view, cx) > 1. {
        redraw(&view, cx);
        let now = behind(&view, cx);
        assert!(now <= last, "only ever toward the end");
        last = now;
        frames += 1;
        assert!(frames < 2000, "reaches the end");
    }
    assert!(frames > 1, "over several frames");
}

#[gpui::test]
fn scrolled_up_the_list_stays_while_output_arrives(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entries = (1..=40)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    let mut stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    redraw(&view, cx);
    // The user scrolls up with the wheel.
    let chat = cx.debug_bounds("chat-view").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: chat.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(200.))),
        ..Default::default()
    });
    redraw(&view, cx);
    assert!(behind(&view, cx) > 100., "scrolled up");
    let top = view.read_with(cx, |view, _| {
        view.chats
            .pane("w1:p1")
            .unwrap()
            .list
            .scroll_px_offset_for_scrollbar()
            .y
    });

    send(
        &mut stream,
        &change(append(40, &"\n\nMore.".repeat(8)), cursor(1, 1)),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let entries = &view
                .chats
                .pane("w1:p1")
                .unwrap()
                .session
                .transcript()
                .entries;
            matches!(&entries[39].kind, EntryKind::Agent { text } if text.len() > 40)
        })
    });
    for _ in 0..20 {
        redraw(&view, cx);
    }
    let after = view.read_with(cx, |view, _| {
        view.chats
            .pane("w1:p1")
            .unwrap()
            .list
            .scroll_px_offset_for_scrollbar()
            .y
    });
    assert_eq!(after, top, "the reader's place holds");
}

#[gpui::test]
fn a_shorter_list_catches_up_to_its_end_on_its_own(cx: &mut TestAppContext) {
    let (view, _peer, cx) = gliding(cx);
    let program = Program::new();
    let entries = (1..=40)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    redraw(&view, cx);
    assert!(behind(&view, cx) <= 1.);
    // A long draft takes room from the list, which keeps its top: the last
    // lines slide under the composer until the glide brings them back.
    view.update(cx, |view, cx| {
        view.chats.pane_mut("w1:p1").unwrap().draft = vec!["more".into(); 5];
        cx.notify();
    });
    cx.run_until_parked();
    // Only the frames the list asks for itself, no other redraw.
    for _ in 0..2000 {
        if behind(&view, cx) <= 1. {
            return;
        }
        let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
        assert!(asked > 0, "behind by {} without a frame", behind(&view, cx));
        cx.run_until_parked();
    }
    panic!("still {} behind", behind(&view, cx));
}

#[gpui::test]
fn a_sent_prompt_shows_whole_with_its_delivered_note(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entries = (1..=40)
        .map(|id| agent(id, "A line of an earlier reply."))
        .collect();
    let mut stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    view.update(cx, |view, cx| {
        view.set_chat_layout_with(crate::config::ChatLayoutMode::IMessage, |_| Ok(()), cx)
    });
    redraw(&view, cx);
    send(
        &mut stream,
        &change(
            Change::Push {
                entry: Entry {
                    id: EntryId(41),
                    kind: EntryKind::User {
                        text: "another".into(),
                    },
                },
            },
            cursor(1, 1),
        ),
    );
    send(
        &mut stream,
        &change(
            Change::Status {
                status: Status::Working,
            },
            cursor(1, 2),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .unwrap()
                .session
                .transcript()
                .status
                == Status::Working
        })
    });
    for _ in 0..500 {
        let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        if asked == 0 {
            break;
        }
    }
    let delivered = cx.debug_bounds("chat-delivered").unwrap();
    let typing = cx.debug_bounds("chat-typing").unwrap();
    assert!(
        delivered.bottom() <= typing.top(),
        "{delivered:?} above {typing:?}"
    );
}

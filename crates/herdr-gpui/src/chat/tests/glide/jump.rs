//! The jump control never moves the reader until explicitly clicked.

use super::*;
use crate::{
    chat::selection::{Caret, Cell, Region},
    config::ChatLayoutMode,
};
use gpui::{AssetSource, Modifiers, ScrollDelta, ScrollWheelEvent, point, px, size};

fn scroll_up(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    let position = view.read_with(cx, |view, _| {
        view.chats
            .pane("w1:p1")
            .unwrap()
            .list
            .viewport_bounds()
            .center()
    });
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.), px(200.))),
        ..Default::default()
    });
    redraw(view, cx);
    assert!(behind(view, cx) > 100.);
}

fn jump(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    assert!(
        crate::icons::Icons
            .load("icons/arrow-down.svg")
            .unwrap()
            .is_some(),
        "the jump arrow must be packaged, not only allocated layout space"
    );
    let button = cx.debug_bounds("chat-jump-latest").unwrap();
    let viewport = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.viewport_bounds()
    });
    assert!(button.left() >= viewport.left() && button.right() <= viewport.right());
    assert!(button.top() >= viewport.top() && button.bottom() <= viewport.bottom());
    assert!((button.center().x - viewport.center().x).abs() <= px(1.));
    let arrow = cx.debug_bounds("chat-jump-arrow").unwrap();
    let label = cx.debug_bounds("chat-jump-label").unwrap();
    assert!((arrow.center().x - button.center().x).abs() <= px(1.));
    assert!((label.center().x - button.center().x).abs() <= px(1.));
    assert!(arrow.top() >= label.bottom());
    if let Some(composer) = cx.debug_bounds("chat-composer") {
        assert!(button.bottom() < composer.top());
    }
    // The full-width centering layer must not swallow scrolling beside the pill.
    let before = behind(view, cx);
    cx.simulate_event(ScrollWheelEvent {
        position: point(viewport.left() + px(2.), button.center().y),
        delta: ScrollDelta::Pixels(point(px(0.), px(40.))),
        ..Default::default()
    });
    redraw(view, cx);
    assert!(behind(view, cx) > before + 1.);
    let button = cx.debug_bounds("chat-jump-latest").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    redraw(view, cx);
    assert!(
        behind(view, cx) <= 1.,
        "explicit jump reaches the latest output"
    );
    assert!(cx.debug_bounds("chat-jump-latest").is_none());
}

#[gpui::test]
fn every_layout_holds_new_output_until_jump_and_keeps_the_composer(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entries = (1..=40).map(|id| agent(id, "An earlier reply.")).collect();
    let mut stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    let composer = cx.debug_bounds("chat-composer").unwrap();
    cx.simulate_click(composer.center(), Modifiers::default());
    cx.simulate_input("unsent café");

    for (index, &layout) in ChatLayoutMode::ALL.iter().enumerate() {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx);
            let pane = view.chats.pane_mut("w1:p1").unwrap();
            pane.glide.reset(&pane.list);
        });
        poll(&view, &program, cx);
        assert!(cx.debug_bounds("chat-jump-latest").is_none());
        scroll_up(&view, cx);
        let top = view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().list.logical_scroll_top()
        });
        let id = 41 + index as u64;
        let seq = index as u64 * 2 + 1;
        send(
            &mut stream,
            &change(
                Change::Push {
                    entry: agent(id, "New output."),
                },
                cursor(1, seq),
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
                    .entries
                    .last()
                    .unwrap()
                    .id
                    == EntryId(id)
            })
        });
        let after = view.read_with(cx, |view, _| {
            view.chats.pane("w1:p1").unwrap().list.logical_scroll_top()
        });
        assert_eq!(
            (after.item_ix, after.offset_in_item),
            (top.item_ix, top.offset_in_item)
        );
        jump(&view, cx);
        assert!(cx.update(|window, cx| {
            let pane = view.read(cx).chats.pane("w1:p1").unwrap();
            let input = pane.composer.as_ref().unwrap().read(cx);
            assert_eq!(input.text(), "unsent café");
            input.focus.is_focused(window)
        }));
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
            &change(append(id, "\n\nMore output."), cursor(1, seq + 1)),
        );
        wait_until(|| {
            poll(&view, &program, cx);
            view.read_with(cx, |view, _| {
                matches!(&view.chats.pane("w1:p1").unwrap().session.transcript().entries.last().unwrap().kind,
                    EntryKind::Agent { text } if text.ends_with("More output."))
            })
        });
        redraw(&view, cx);
        assert!(behind(&view, cx) <= 1., "{layout:?} resumes tail following");
    }
}

#[gpui::test]
fn a_narrow_read_only_selection_can_jump_and_a_new_snapshot_resets_the_control(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = gliding(cx);
    cx.simulate_resize(size(px(650.), px(860.)));
    cx.run_until_parked();
    let program = Program::new();
    let mut message = snapshot(
        cursor(1, 0),
        (1..=40).map(|id| agent(id, "An earlier reply.")).collect(),
    );
    if let ServerMessage::Snapshot { transcript, .. } = &mut message {
        transcript.read_only = true;
    }
    let mut stream = attach_with(&view, &program, message, cx);
    selection::acknowledge_size(&view, cx);
    poll(&view, &program, cx);
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        let doc = pane.prepared.text();
        let key = doc
            .key(EntryId(1), Region::Body(0), Cell::default())
            .unwrap();
        pane.selection
            .press(doc, Caret { key, offset: 0 }, 1, false);
        pane.selection.drag(doc, Caret { key, offset: 5 });
        pane.selection.release();
        cx.notify();
    });
    let top = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.logical_scroll_top()
    });
    send(
        &mut stream,
        &change(append(40, &"\n\nMore output.".repeat(10)), cursor(1, 1)),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        // The first layout measures the growing row; the following frame
        // paints the button without advancing the selected reading position.
        behind(&view, cx) > 20. && cx.debug_bounds("chat-jump-latest").is_some()
    });
    view.read_with(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        let after = pane.list.logical_scroll_top();
        assert_eq!(
            (after.item_ix, after.offset_in_item),
            (top.item_ix, top.offset_in_item)
        );
        assert_eq!(
            pane.selection.text(pane.prepared.text()).as_deref(),
            Some("An ea")
        );
    });
    jump(&view, cx);
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert!(pane.selection.is_empty());
        assert!(pane.session.resend().is_empty());
    });
    scroll_up(&view, cx);
    assert!(cx.debug_bounds("chat-jump-latest").is_some());
    send(
        &mut stream,
        &snapshot(cursor(2, 0), vec![agent(1, "A new transcript.")]),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .unwrap()
                .session
                .transcript()
                .entries
                .len()
                == 1
        })
    });
    redraw(&view, cx);
    assert!(cx.debug_bounds("chat-jump-latest").is_none());
}

//! The preference changes actual row geometry without throwing away selection.
use super::{
    overlay::{attach_with, open, poll},
    selection::{acknowledge_size, caret, clipboard, copy_key, drag, position},
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{TestAppContext, px, size};
use herdr_client::ConnectTarget;

const TEXT: &str = "Read naïve café 中文 text across a paragraph that remains selectable when the reading width changes. Longer lines can use the available window, while narrow panes continue to wrap safely.";

#[gpui::test]
fn every_layout_honors_default_custom_and_full_width_and_clamps_to_a_narrow_pane(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| {
        view.sidebar_width = Some(150.);
        view.config.sidebar_style.activity_bar = false;
    });
    cx.simulate_resize(size(px(1800.), px(860.)));
    cx.run_until_parked();
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, TEXT)]),
        cx,
    );
    acknowledge_size(&view, cx);
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        let mut measured = Vec::new();
        for width in [800, 1200, 0] {
            view.update(cx, |view, _| view.config.chat.max_width = width);
            poll(&view, &program, cx);
            let column = cx.debug_bounds("chat-reading-column").unwrap();
            let prose = cx.debug_bounds("chat-prose").unwrap();
            let pane = cx.debug_bounds("chat-view").unwrap();
            assert!(
                column.left() >= pane.left() && column.right() <= pane.right(),
                "{layout:?}: {column:?} in {pane:?}"
            );
            if width != 0 {
                assert!(
                    (f32::from(column.size.width) - f32::from(width)).abs() < 2.,
                    "{layout:?}: requested {width}, got {column:?}"
                );
            } else {
                assert!(
                    column.size.width > px(1400.),
                    "{layout:?}: full width must exceed the custom limit"
                );
            }
            measured.push(prose.size.width);
        }
        assert!(
            measured[1] > measured[0] + px(200.),
            "{layout:?}: message text must expand too: {measured:?}"
        );
        assert!(
            measured[2] > measured[1],
            "{layout:?}: full width reaches the message row"
        );
    }
    cx.simulate_resize(size(px(650.), px(860.)));
    cx.run_until_parked();
    acknowledge_size(&view, cx);
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        let mut measured = Vec::new();
        for width in [800, 1200, 0] {
            view.update(cx, |view, _| view.config.chat.max_width = width);
            poll(&view, &program, cx);
            let column = cx.debug_bounds("chat-reading-column").unwrap();
            let pane = cx.debug_bounds("chat-view").unwrap();
            let prose = cx.debug_bounds("chat-prose").unwrap();
            assert!(
                column.size.width <= pane.size.width && column.size.width < px(500.),
                "{layout:?}: {column:?}"
            );
            assert!(
                prose.left() >= pane.left() && prose.right() <= pane.right(),
                "{layout:?}: prose escaped the narrow pane"
            );
            measured.push(column.size.width);
        }
        assert!(
            measured
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < px(2.)),
            "{layout:?}: oversized limits must all clamp to the same pane"
        );
    }
}

#[gpui::test]
fn changing_reading_width_reflows_text_and_preserves_its_selection(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    cx.simulate_resize(size(px(1800.), px(860.)));
    cx.run_until_parked();
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, TEXT)]),
        cx,
    );
    acknowledge_size(&view, cx);
    poll(&view, &program, cx);
    let from = position(&view, caret(&view, 0, 0, 0, "Read ".len(), cx), cx);
    let to = position(
        &view,
        caret(&view, 0, 0, 0, "Read naïve café 中文".len(), cx),
        cx,
    );
    drag(cx, from, to);
    let mut widths = Vec::new();
    for width in [800, 1200, 0, 640] {
        view.update(cx, |view, _| view.config.chat.max_width = width);
        poll(&view, &program, cx);
        cx.simulate_keystrokes(copy_key());
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("naïve café 中文"),
            "width={width}"
        );
        widths.push(cx.debug_bounds("chat-prose").unwrap().size.width);
    }
    assert!(
        widths[0] < widths[1] && widths[1] < widths[2] && widths[3] < widths[0],
        "{widths:?}"
    );
}

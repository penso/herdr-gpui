//! Real mouse/key events exercise the shaped rich text and input routing.
use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{HerdrWindow, chat::selection::Caret, config::ChatLayoutMode};
use gpui::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, px, size,
};
use herdr_client::{ConnectTarget, protocol::SurfaceRect};

mod routing;

const PROSE: &str = "Read naïve café 中文 text across a narrow paragraph. These extra words deliberately wrap onto several lines before the code and table below.";
const SOURCE: &str = "Read naïve café 中文 text across a narrow paragraph. These extra words deliberately wrap onto several lines before the code and table below.\n\n```rust\n\tlet α = 1;\n\tprintln!(\"{α}\");\n```\n\n| Name | Value |\n|:--|--:|\n| café | 42 |";

pub(super) fn copy_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-c"
    }
}

fn select_all_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    }
}

pub(super) fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

pub(super) fn caret(
    view: &Entity<HerdrWindow>,
    block: usize,
    row: usize,
    column: usize,
    offset: usize,
    cx: &VisualTestContext,
) -> Caret {
    view.read_with(cx, |view, _| {
        let document = view.chats.pane("w1:p1").unwrap().prepared.text();
        Caret {
            key: document
                .key(
                    EntryId(1),
                    crate::chat::selection::Region::Body(0),
                    crate::chat::selection::Cell { block, row, column },
                )
                .unwrap(),
            offset,
        }
    })
}

pub(super) fn position(
    view: &Entity<HerdrWindow>,
    caret: Caret,
    cx: &VisualTestContext,
) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        view.chats
            .pane("w1:p1")
            .unwrap()
            .text_geometry
            .position(caret.key, caret.offset)
            .unwrap_or_else(|| panic!("no shaped position for {caret:?}"))
    })
}

pub(super) fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
}

fn narrow(view: &Entity<HerdrWindow>, layout: ChatLayoutMode, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.config.chat.layout = layout;
        view.sidebar_width = Some(150.);
        view.config.sidebar_style.activity_bar = false;
        cx.notify();
    });
    cx.simulate_resize(size(px(650.), px(860.)));
    cx.run_until_parked();
}

pub(super) fn acknowledge_size(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.frame.width = view.options.surface_size.cols;
        surface.frame.height = view.options.surface_size.rows;
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width: surface.frame.width,
            height: surface.frame.height,
        };
        surface.panes[0].rect = rect;
        surface.panes[0].inner_rect = rect;
    });
}

#[gpui::test]
fn wrapped_unicode_prose_code_and_table_copy_in_both_drag_directions(cx: &mut TestAppContext) {
    for layout in [
        ChatLayoutMode::Bubbles,
        ChatLayoutMode::Document,
        ChatLayoutMode::IMessage,
    ] {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        narrow(&view, layout, cx);
        let program = Program::new();
        let _stream = attach_with(
            &view,
            &program,
            snapshot(cursor(1, 0), vec![agent(1, SOURCE)]),
            cx,
        );
        acknowledge_size(&view, cx);
        poll(&view, &program, cx);
        assert!(cx.debug_bounds("announcement").is_none());
        assert!(cx.debug_bounds("config-diagnostic").is_none());
        let start = position(&view, caret(&view, 0, 0, 0, "Read ".len(), cx), cx);
        let paragraph_end = position(&view, caret(&view, 0, 0, 0, PROSE.len(), cx), cx);
        assert!(
            paragraph_end.y > start.y + px(20.),
            "{layout:?}: prose must actually wrap"
        );
        let end = position(&view, caret(&view, 2, 1, 1, 2, cx), cx);
        let cell = cx.debug_bounds("chat-table-cell-1-1").unwrap();
        assert!(
            end.x > cell.right() - px(24.),
            "{layout:?}: aligned text caret {end:?} must follow the painted right edge {cell:?}"
        );
        let expected = format!(
            "{}\n\n\tlet α = 1;\n\tprintln!(\"{{α}}\");\n\nName\tValue\ncafé\t42",
            &PROSE["Read ".len()..]
        );
        for (from, to) in [(start, end), (end, start)] {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("not copied".into()));
            drag(cx, from, to);
            let (selected, focused, detail) = cx.update(|window, cx| {
                let pane = view.read(cx).chats.pane("w1:p1").unwrap();
                (
                    pane.selection.text(pane.prepared.text()),
                    pane.text_focus
                        .as_ref()
                        .is_some_and(|focus| focus.is_focused(window)),
                    format!(
                        "viewport={:?}, hit_from={:?}, hit_to={:?}, selecting={}",
                        pane.list.viewport_bounds(),
                        pane.text_geometry.hit(from),
                        pane.text_geometry.hit(to),
                        pane.selection.is_selecting()
                    ),
                )
            });
            assert_eq!(
                selected.as_deref(),
                Some(expected.as_str()),
                "{layout:?}: drag {from:?} to {to:?}; {detail}"
            );
            assert!(
                focused,
                "{layout:?}: reader must retain focus after mouse-up; {detail}"
            );
            cx.simulate_keystrokes(copy_key());
            assert_eq!(
                clipboard(cx).as_deref(),
                Some(expected.as_str()),
                "{layout:?}"
            );
            view.read_with(cx, |view, _| {
                let pane = view.chats.pane("w1:p1").unwrap();
                assert!(
                    !pane.selection.is_selecting(),
                    "mouse-up finishes the gesture"
                );
                assert_eq!(
                    pane.selection.text(pane.prepared.text()).as_deref(),
                    Some(expected.as_str())
                );
            });
        }
    }
}

#[gpui::test]
fn inline_and_display_math_copy_the_original_tex_atomically(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![agent(1, "Before $x^2$ after.\n\n$$\\sum_{i=1}^n i$$")],
        ),
        cx,
    );
    wait_until(|| {
        cx.run_until_parked();
        poll(&view, &program, cx);
        cx.debug_bounds("chat-math-inline").is_some()
            && cx.debug_bounds("chat-math-display").is_some()
    });
    for (selector, expected) in [
        ("chat-math-inline", "x^2"),
        ("chat-math-display", "\\sum_{i=1}^n i"),
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        let from = gpui::point(bounds.left() + bounds.size.width * 0.25, bounds.center().y);
        let to = gpui::point(bounds.left() + bounds.size.width * 0.75, bounds.center().y);
        drag(cx, from, to);
        cx.simulate_keystrokes(copy_key());
        assert_eq!(clipboard(cx).as_deref(), Some(expected));
    }
}

#[gpui::test]
fn dragging_a_link_selects_its_label_and_a_plain_click_still_opens_it(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![agent(
                1,
                "[Readable reference](https://example.org/selection)",
            )],
        ),
        cx,
    );
    let from = position(&view, caret(&view, 0, 0, 0, 0, cx), cx);
    let to = position(
        &view,
        caret(&view, 0, 0, 0, "Readable reference".len(), cx),
        cx,
    );
    drag(cx, from, to);
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("Readable reference"));
    assert!(
        cx.opened_url().is_none(),
        "a drag must not follow its starting link"
    );
    cx.simulate_keystrokes("escape");
    let click = position(&view, caret(&view, 0, 0, 0, 3, cx), cx);
    cx.simulate_click(click, Modifiers::default());
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://example.org/selection")
    );
}

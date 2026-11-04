use super::*;
use gpui::{MouseButton, MouseDownEvent, Pixels, Point, point, px};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn begin_drag(cx: &mut VisualTestContext, to: Point<Pixels>) {
    let from = cx.debug_bounds("chat-input-resize").unwrap().center();
    cx.simulate_mouse_move(from, None, Modifiers::default());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    draw(cx);
}

#[gpui::test]
fn dragging_original_input_resizes_releases_and_restores_automatic_sizing(cx: &mut TestAppContext) {
    let (view, mut peer, _program, _stream, cx) = observed(cx);
    let bar = cx.debug_bounds("chat-terminal-input").unwrap();
    let line_height = view.read_with(cx, |view, _| view.config.terminal.line_height());
    let original_surface = view.read_with(cx, |view, _| view.options.surface_size);
    let end = bar.origin + point(px(24.), px(-6. * line_height));
    begin_drag(cx, end);
    let grown = cx.debug_bounds("chat-terminal-input").unwrap();
    assert!(grown.top() < bar.top());
    view.read_with(cx, |view, cx| {
        let chat = view.chats.pane("w1:p1").unwrap();
        assert_eq!(chat.terminal_size, InputSize::Rows(14));
        assert!(chat.selection.is_empty());
        assert!(view.selection.is_none());
        assert_eq!(view.options.surface_size, original_surface);
        assert!(cx.has_active_drag());
    });
    assert!(cx.update(|window, cx| view.read(cx).focus.is_focused(window)));

    // Continue after rerender and leave the pane horizontally. The divider
    // follows the same pointer without becoming terminal selection or input.
    let end = point(bar.right() + px(30.), bar.top() + px(4. * line_height));
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    let released = cx.debug_bounds("chat-terminal-input").unwrap();
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Rows(4)
        );
        assert!(!cx.has_active_drag());
    });
    cx.simulate_mouse_move(bar.origin, None, Modifiers::default());
    draw(cx);
    assert_eq!(cx.debug_bounds("chat-terminal-input").unwrap(), released);

    let position = cx.debug_bounds("chat-input-resize").unwrap().center();
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(cx.debug_bounds("chat-terminal-input").unwrap(), bar);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Auto
        );
    });

    // The separate button retains its original behavior after a manual drag.
    let button = cx.debug_bounds("chat-expand-input").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Expanded
        );
        assert!(view.chats.pane("w1:p1").unwrap().selection.is_empty());
    });
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
    });
}

#[gpui::test]
fn an_active_input_drag_cannot_resize_a_replaced_cli_session(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    let bar = cx.debug_bounds("chat-terminal-input").unwrap();
    begin_drag(cx, bar.center() - point(px(0.), px(100.)));
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        pane.target.existing.as_mut().unwrap().session_id = "replacement-session".into();
        pane.terminal_size = InputSize::Compact;
    });
    draw(cx);
    let end = bar.origin - point(px(0.), px(200.));
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.chats.pane("w1:p1").unwrap().terminal_size,
            InputSize::Compact
        );
        assert!(!cx.has_active_drag());
    });
}

#[gpui::test]
fn a_small_terminal_font_keeps_expand_and_compact_outside_the_resize_grip(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = observed(cx);
    view.update(cx, |view, _| view.config.terminal.size = 6.);
    draw(cx);
    for expected in [InputSize::Expanded, InputSize::Compact] {
        let grip = cx.debug_bounds("chat-input-resize").unwrap();
        let button = cx.debug_bounds("chat-expand-input").unwrap();
        assert!(button.center().y < grip.bottom());
        assert!(button.left() >= grip.right());
        cx.simulate_click(button.center(), Modifiers::default());
        draw(cx);
        view.read_with(cx, |view, cx| {
            assert_eq!(view.chats.pane("w1:p1").unwrap().terminal_size, expected);
            assert!(!cx.has_active_drag());
        });
    }
}

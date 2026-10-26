//! Rich conversation zoom keeps both native composers and existing CLI input
//! focused, and claims only its dedicated keys while the rich view is shown.

use super::{
    overlay::{attach_with, open, poll},
    terminal_input::{next_input, observed},
    *,
};
use crate::{
    config::FONT_SIZE_STEP,
    controls::{CTRL_SHORTCUTS, Command},
    sidebar::layout_tests::full_draw,
};
use gpui::{Modifiers, TestAppContext};
use herdr_client::{
    ConnectTarget,
    protocol::{ClientKeyCode, ClientPaneInputEvent},
};

#[gpui::test]
fn zoom_shortcuts_keep_the_native_chat_composer_focused(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    cx.update(|_, cx| crate::bind_keys(cx));
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, "Readable prose.")]),
        cx,
    );
    cx.simulate_input("keep this draft");
    let input = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().composer.clone().unwrap()
    });
    let start = view.read_with(cx, |view, _| view.config.terminal.size);
    let mut shortcuts = vec![
        (
            crate::controls::platform_default("cmd-="),
            start + FONT_SIZE_STEP,
        ),
        (crate::controls::platform_default("cmd--"), start),
        (
            crate::controls::platform_default("cmd-+"),
            start + FONT_SIZE_STEP,
        ),
        (crate::controls::platform_default("cmd-0"), start),
    ];
    if CTRL_SHORTCUTS {
        shortcuts.extend([
            ("ctrl--", start - FONT_SIZE_STEP),
            ("ctrl-=", start),
            ("ctrl-+", start + FONT_SIZE_STEP),
            ("ctrl-0", start),
        ]);
    }
    if cfg!(target_os = "linux") {
        // Actual Linux events for Ctrl-Shift--, Ctrl-Shift-=, Ctrl-Shift-0.
        shortcuts.extend([
            ("ctrl-_", start - FONT_SIZE_STEP),
            ("ctrl-+", start),
            ("ctrl-)", start),
        ]);
    }
    for (key, expected) in shortcuts {
        cx.simulate_keystrokes(key);
        // Check before polling: zoom itself must preserve focus, without
        // waiting for follow_chat_focus to repair a stolen composer.
        cx.update(|window, cx| {
            assert!(input.read(cx).focus.is_focused(window), "{key}");
            view.read_with(cx, |view, _| {
                assert_eq!(view.config.terminal.size, expected, "{key}");
                assert_eq!(view.chat_text_size(), expected + 2., "{key}");
            });
        });
        poll(&view, &program, cx);
    }
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "keep this draft"
    );
}

#[gpui::test]
fn rich_zoom_works_from_the_conversation_and_original_terminal_without_typing_keys(
    cx: &mut TestAppContext,
) {
    if !CTRL_SHORTCUTS {
        return;
    }
    let (view, mut peer, _program, _stream, cx) = observed(cx);
    cx.update(|_, cx| crate::bind_keys(cx));
    let start = view.read_with(cx, |view, _| view.config.terminal.size);
    for selector in ["chat-view", "chat-terminal-input"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), Modifiers::default());
        let mut shortcuts = vec![
            ("ctrl--", start - FONT_SIZE_STEP),
            ("ctrl-=", start),
            ("ctrl-+", start + FONT_SIZE_STEP),
            ("ctrl-0", start),
        ];
        if cfg!(target_os = "linux") {
            shortcuts.extend([
                ("ctrl-_", start - FONT_SIZE_STEP),
                ("ctrl-+", start),
                ("ctrl-)", start),
            ]);
        }
        for (key, expected) in shortcuts {
            cx.simulate_keystrokes(key);
            cx.run_until_parked();
            cx.update(|window, cx| {
                let view = view.read(cx);
                assert_eq!(view.config.terminal.size, expected, "{selector}: {key}");
                assert!(view.focus.is_focused(window));
                assert!(!view.chats.raw("w1:p1"));
            });
        }
        // Any leaked zoom key would precede this ordinary CLI shortcut.
        cx.simulate_keystrokes("ctrl-c");
        assert!(matches!(
            next_input(&mut peer).as_slice(),
            [ClientPaneInputEvent::Key {
                code: ClientKeyCode::Char('c'),
                modifiers: 2,
                ..
            }]
        ));
    }
    // The observer never receives a synthetic prompt or cancellation.
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
fn rich_zoom_leaves_raw_terminals_and_explicit_pane_bindings_alone(cx: &mut TestAppContext) {
    if !CTRL_SHORTCUTS {
        return;
    }
    let (view, mut peer, _program, _stream, cx) = observed(cx);
    let start = view.read_with(cx, |view, _| view.config.terminal.size);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.set_chat_raw("w1:p1", true, window, cx));
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_keystrokes("ctrl--");
    assert!(matches!(
        next_input(&mut peer).as_slice(),
        [ClientPaneInputEvent::Key {
            code: ClientKeyCode::Char('-'),
            modifiers: 2,
            ..
        }]
    ));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_chat_raw("w1:p1", false, window, cx);
            view.config.keybindings = crate::keymap::Keymap::with_overrides(
                &Default::default(),
                &[("ctrl--".into(), "ctrl-u".into())].into(),
                &crate::keymap::DaemonKeys::default(),
            )
            .unwrap();
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_keystrokes("ctrl--");
    assert!(matches!(
        next_input(&mut peer).as_slice(),
        [ClientPaneInputEvent::Key {
            code: ClientKeyCode::Char('u'),
            modifiers: 2,
            ..
        }]
    ));
    view.read_with(cx, |view, _| assert_eq!(view.config.terminal.size, start));
}

#[gpui::test]
fn font_commands_resize_rich_rows_and_reset_them(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![agent(1, "First paragraph.\n\nSecond paragraph.")],
        ),
        cx,
    );
    let before = cx.debug_bounds("chat-prose").unwrap().size.height;
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for _ in 0..4 {
                view.command(Command::IncreaseFontSize, window, cx);
            }
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("chat-prose").unwrap().size.height > before);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ResetFontSize, window, cx)
        });
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(cx.debug_bounds("chat-prose").unwrap().size.height, before);
}

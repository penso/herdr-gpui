use super::*;
use std::collections::BTreeMap;

/// A window with a focused pane ready for input, `ctrl-shift-y` bound to a
/// new tab, and pass-through on.
fn passing_window(
    cx: &mut gpui::TestAppContext,
) -> (
    gpui::Entity<SheprdWindow>,
    Server,
    &mut gpui::VisualTestContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, server) = connected_endpoint("pass-through");
    let overrides = BTreeMap::from([
        (
            "new_tab".to_owned(),
            crate::keymap::Binding::One("ctrl-shift-y".to_owned()),
        ),
        (
            "toggle_pass_through".to_owned(),
            crate::keymap::Binding::One("ctrl-alt-y".to_owned()),
        ),
    ]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            view.live.activation = Some(crate::state::SurfaceActivation {
                request: "activate-1".into(),
                boot: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
                revision: Some(view.live.surface.as_ref().unwrap().projection_revision),
                failed: false,
                focus: None,
                active: true,
            });
            view.config.keybindings = crate::keymap::Keymap::with_overrides(
                &overrides,
                &Default::default(),
                &Default::default(),
            )
            .unwrap();
            window.focus(&view.focus.clone(), cx);
            view.command(Command::TogglePassThrough, window, cx);
        });
    });
    (view, server, cx)
}

fn is_key(message: &ClientMessage, expected: char, expected_modifiers: u8) -> bool {
    let ClientMessage::ClientShellPaneInput { events, .. } = message else {
        return false;
    };
    matches!(
        events.as_slice(),
        [ClientPaneInputEvent::Key { code: ClientKeyCode::Char(code), modifiers, .. }]
            if code.eq_ignore_ascii_case(&expected) && *modifiers == expected_modifiers
    )
}

fn sentinel(view: &gpui::Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
    });
}

fn is_sentinel(message: &ClientMessage) -> bool {
    matches!(
        message,
        ClientMessage::ClientShellPaneInput { events, .. }
            if events.as_slice() == [ClientPaneInputEvent::TextCommit("sentinel".into())]
    )
}

/// The app's binding would run first, so the key is handed to the pane, once.
#[gpui::test]
fn a_key_the_app_binds_reaches_the_pane_once(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = passing_window(cx);
    let handled = cx.update(|window, cx| {
        window.dispatch_keystroke(gpui::Keystroke::parse("ctrl-shift-y").unwrap(), cx)
    });
    assert!(handled, "it stops here, before the binding");
    sentinel(&view, cx);
    let message = server.receive();
    assert!(is_key(&message, 'y', 3), "{message:?}");
    let message = server.receive();
    assert!(is_sentinel(&message), "{message:?}");
}

/// Ctrl-Shift-V pastes otherwise; here it is the program's key.
#[gpui::test]
fn the_apps_paste_key_goes_to_the_pane(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = passing_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let event = gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("ctrl-shift-v").unwrap(),
                is_held: false,
                prefer_character_input: false,
            };
            view.key_down(&event, window, cx);
        });
    });
    sentinel(&view, cx);
    let message = server.receive();
    assert!(is_key(&message, 'v', 3), "{message:?}");
}

/// No pane can receive Cmd-V, so it would be lost if it left the app's path.
#[gpui::test]
fn a_key_no_pane_can_receive_keeps_its_app_meaning(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = passing_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
            view.key_down(&file_paste::cmd_v(), window, cx);
            assert!(view.pass_through, "and the mode stays on");
        });
    });
    cx.run_until_parked();
    sentinel(&view, cx);
    let message = server.receive();
    assert!(
        matches!(
            &message,
            ClientMessage::ClientShellPaneInput { events, .. }
                if events.as_slice() == [ClientPaneInputEvent::Paste("pasted".into())]
        ),
        "{message:?}"
    );
    let message = server.receive();
    assert!(is_sentinel(&message), "{message:?}");
    wait_image_finished(&view, cx);
}

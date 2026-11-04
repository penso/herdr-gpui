use super::*;

/// F20 as `hidutil` delivers it: no name, and no character.
fn unnamed() -> gpui::Keystroke {
    gpui::Keystroke {
        modifiers: gpui::Modifiers::default(),
        key: String::new(),
        key_char: None,
    }
}

/// F20's key code, read as AppKit's current event would give it.
fn f20() -> Option<(u16, bool)> {
    Some((90, false))
}

fn f20_key(kind: ClientKeyKind, tracks_release: bool) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events: vec![ClientPaneInputEvent::Key {
            code: ClientKeyCode::F(20),
            modifiers: 0,
            kind,
            repeat_count: 1,
            shifted_codepoint: None,
            generated_text: None,
            tracks_release,
            physical_key_id: None,
            windows_record: None,
        }],
    }
}

fn sentinel() -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events: vec![ClientPaneInputEvent::TextCommit("sentinel".into())],
    }
}

/// A window whose prefix is F20, with a focused pane ready for input and the
/// key code of every unnamed keystroke read as F20's.
fn f20_window(
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
    let (endpoint, server) = connected_endpoint("function-keys");
    cx.update(|_, cx| {
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
            let keys: toml::Table = "prefix = \"f20\"".parse().unwrap();
            view.config.keybindings = crate::keymap::Keymap::with_overrides(
                &Default::default(),
                &Default::default(),
                &crate::keymap::DaemonKeys::from_table(Some(&keys)),
            )
            .unwrap();
            view.key_press = f20;
        });
    });
    (view, server, cx)
}

/// Whether the window's handlers stopped the keystroke, as the prefix does.
fn press(cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| window.dispatch_keystroke(unnamed(), cx))
}

/// An F20 that arrives without its name arms an F20 prefix, and the second
/// one disarms it and goes on to the pane as F20, once.
#[gpui::test]
fn an_unnamed_function_key_is_the_prefix_and_its_second_press_reaches_the_pane(
    cx: &mut gpui::TestAppContext,
) {
    let (view, mut server, cx) = f20_window(cx);
    assert!(press(cx), "the first press is the prefix's");
    assert!(view.read_with(cx, |view, _| view.prefix_armed));
    assert!(!press(cx), "the second press goes on to the focused pane");
    assert!(!view.read_with(cx, |view, _| view.prefix_armed));
    // The fixture keeps the window out of the render tree, so the pane's key
    // handler, which GPUI calls next for the press it let through, is called
    // here; the first press never reaches it.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let event = gpui::KeyDownEvent {
                keystroke: unnamed(),
                is_held: false,
                prefer_character_input: false,
            };
            view.key_down(&event, window, cx);
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
    });
    assert_eq!(server.receive(), f20_key(ClientKeyKind::Press, false));
    assert_eq!(server.receive(), sentinel());
}

/// While Herdr reports that the pane wants every key, an unnamed F20 is
/// pressed and released under its name.
#[gpui::test]
fn an_unnamed_function_key_is_released_under_its_name(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = f20_window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.keyboard_report_all = true;
            let down = gpui::KeyDownEvent {
                keystroke: unnamed(),
                is_held: false,
                prefer_character_input: false,
            };
            view.key_down(&down, window, cx);
            view.key_up(
                &gpui::KeyUpEvent {
                    keystroke: unnamed(),
                },
                window,
                cx,
            );
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
    });
    assert_eq!(server.receive(), f20_key(ClientKeyKind::Press, true));
    assert_eq!(server.receive(), f20_key(ClientKeyKind::Release, true));
    assert_eq!(server.receive(), sentinel());
}

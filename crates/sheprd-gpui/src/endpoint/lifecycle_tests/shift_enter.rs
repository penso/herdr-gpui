use super::*;

/// Check the semantic events on the client socket, before the daemon encodes
/// them for the pane's keyboard protocol. Report-all alone does not identify
/// that protocol: Shift-Enter must keep its modifiers with either value (#575).
#[gpui::test]
fn shift_enter_stays_distinct_on_the_wire_and_honors_explicit_overrides(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("shift-enter");
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
            assert!(view.input_ready());
        });
    });
    for report_all in [false, true] {
        // The empty value was the opt-out for #518 and must remain valid.
        for (target, expected) in [
            (None, (ClientKeyCode::Enter, 1)),
            (Some(""), (ClientKeyCode::Enter, 1)),
            (Some("alt-enter"), (ClientKeyCode::Enter, 4)),
            (Some("ctrl-j"), (ClientKeyCode::Char('j'), 2)),
            (Some("text:\n"), (ClientKeyCode::Enter, 0)),
        ] {
            cx.update(|_, cx| {
                view.update(cx, |view, _| {
                    let pane_keys = target
                        .map(|target| ("shift-enter".into(), target.into()))
                        .into_iter()
                        .collect();
                    view.config.keybindings = crate::keymap::Keymap::with_overrides(
                        &Default::default(),
                        &pane_keys,
                        &crate::keymap::DaemonKeys::default(),
                    )
                    .unwrap();
                    view.live.keyboard_report_all = report_all;
                });
            });
            for (typed, (code, modifiers)) in [
                ("shift-enter", expected.clone()),
                ("alt-enter", (ClientKeyCode::Enter, 4)),
                ("ctrl-enter", (ClientKeyCode::Enter, 2)),
                ("enter", (ClientKeyCode::Enter, 0)),
                ("ctrl-shift-enter", (ClientKeyCode::Enter, 3)),
            ] {
                for is_held in [false, true] {
                    cx.update(|window, cx| {
                        view.update(cx, |view, cx| {
                            view.key_down(
                                &gpui::KeyDownEvent {
                                    keystroke: gpui::Keystroke::parse(typed).unwrap(),
                                    is_held,
                                    prefer_character_input: false,
                                },
                                window,
                                cx,
                            );
                        });
                    });
                    let event = if typed == "shift-enter" && target == Some("text:\n") {
                        ClientPaneInputEvent::TextCommit("\n".into())
                    } else {
                        ClientPaneInputEvent::Key {
                            code: code.clone(),
                            modifiers,
                            kind: if is_held {
                                ClientKeyKind::Repeat
                            } else {
                                ClientKeyKind::Press
                            },
                            repeat_count: 1,
                            shifted_codepoint: None,
                            generated_text: None,
                            tracks_release: report_all,
                            physical_key_id: None,
                            windows_record: None,
                        }
                    };
                    assert_eq!(
                        server.receive(),
                        ClientMessage::ClientShellPaneInput {
                            pane_id: "w1:p1".into(),
                            events: vec![event],
                        },
                        "{typed}, override={target:?}, report_all={report_all}, held={is_held}"
                    );
                }
            }
        }
    }
}

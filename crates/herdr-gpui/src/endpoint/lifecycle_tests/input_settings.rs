//! Herdr's shared `[ui]` input settings, applied where the GUI handles input.

use super::*;
use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase};
use herdr_client::protocol::PaneSurfaceScrollMetrics;

fn shared(view: &mut HerdrWindow, text: &str) {
    view.settings.shared = Some(crate::herdr_settings::Settings::parse_text(text).unwrap());
}

/// One discrete wheel notch, whatever line count the platform reports for it.
fn notch(view: &HerdrWindow, notches: f32) -> ScrollWheelEvent {
    ScrollWheelEvent {
        position: mouse_position(view, 3.5, 4.5),
        delta: ScrollDelta::Lines(point(
            0.,
            notches * f32::from(crate::terminal::PLATFORM_NOTCH_LINES),
        )),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    }
}

fn wheel_event(kind: ClientMouseKind, count: u16) -> ClientPaneInputEvent {
    let mut event = mouse_event(kind, 2, 3);
    if let ClientPaneInputEvent::Mouse { lines, .. } = &mut event {
        *lines = count;
    }
    event
}

/// Herdr's `ui.right_click_passthrough_modifier` sends a right-click held
/// with exactly that combination to the application, stripped of it for the
/// whole gesture, while any other combination still opens the pane menu.
#[gpui::test]
fn connected_right_click_passthrough_modifier_reaches_the_application_stripped(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:mouse");
    let control = gpui::Modifiers {
        control: true,
        ..Default::default()
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            let right = |view: &HerdrWindow, modifiers| MouseDownEvent {
                position: mouse_position(view, 3.5, 4.5),
                button: MouseButton::Right,
                modifiers,
                ..Default::default()
            };
            // Unconfigured, a modified right-click opens the menu.
            assert!(!view.terminal_mouse_down(&right(view, control), window, cx));
            shared(view, "[ui]\nright_click_passthrough_modifier = 'ctrl'");
            for other in [
                gpui::Modifiers::default(),
                gpui::Modifiers {
                    alt: true,
                    ..Default::default()
                },
                gpui::Modifiers {
                    control: true,
                    alt: true,
                    ..Default::default()
                },
            ] {
                assert!(!view.terminal_mouse_down(&right(view, other), window, cx));
                assert!(view.terminal_mouse.is_none());
            }
            assert!(view.terminal_mouse_down(&right(view, control), window, cx));
            assert!(view.terminal_mouse_move(
                &MouseMoveEvent {
                    position: mouse_position(view, 5.5, 6.5),
                    pressed_button: Some(MouseButton::Right),
                    modifiers: gpui::Modifiers {
                        control: true,
                        alt: true,
                        ..Default::default()
                    },
                },
                cx
            ));
            assert!(view.terminal_mouse_up(
                &MouseUpEvent {
                    position: mouse_position(view, 5.5, 6.5),
                    button: MouseButton::Right,
                    modifiers: control,
                    ..Default::default()
                },
                cx
            ));
        });
    });
    let mut drag = mouse_event(ClientMouseKind::Drag(ClientMouseButton::Right), 4, 5);
    if let ClientPaneInputEvent::Mouse { modifiers, .. } = &mut drag {
        // Alt, pressed after the gesture began, still reaches the application.
        *modifiers = 1 << 2;
    }
    for event in [
        mouse_event(ClientMouseKind::Down(ClientMouseButton::Right), 2, 3),
        drag,
        mouse_event(ClientMouseKind::Up(ClientMouseButton::Right), 4, 5),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![event],
            }
        );
    }
}

/// Herdr's `ui.mouse_scroll_lines` sets how far one wheel notch moves
/// scrollback on the whole-row path, taken where a pane cannot slide.
#[gpui::test]
fn connected_wheel_notch_scrolls_the_shared_mouse_scroll_lines(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:wheel");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            // No scroll metrics, so the pane cannot slide; the daemon scrolls
            // its history by the count.
            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].mouse_reporting = false;
            // Herdr's default of three before the shared config loads.
            view.settings.shared = None;
            view.scroll_wheel(&notch(view, 1.), window, cx);
            shared(view, "[ui]\nmouse_scroll_lines = 5");
            view.scroll_wheel(&notch(view, -1.), window, cx);
        });
    });
    for event in [
        wheel_event(ClientMouseKind::ScrollUp, 3),
        wheel_event(ClientMouseKind::ScrollDown, 5),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![event],
            }
        );
    }
}

/// A notch over scrollback slides the configured rows smoothly and asks the
/// daemon for the same count, so smooth scrolling and `mouse_scroll_lines`
/// agree on how far one notch goes.
#[gpui::test]
fn wheel_notch_slides_the_shared_mouse_scroll_lines_through_scrollback(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:wheel");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            let pane = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0];
            pane.mouse_reporting = false;
            pane.scroll = Some(PaneSurfaceScrollMetrics {
                offset_from_bottom: 0,
                max_offset_from_bottom: 1000,
                viewport_rows: 22,
            });
            view.presentation.frame(&view.live);
            shared(view, "[ui]\nmouse_scroll_lines = 5");
            assert!(view.slides_allowed());
            view.scroll_wheel(&notch(view, 1.), window, cx);
            assert!(
                view.presentation.scroll.offset().is_some(),
                "the notch starts a slide"
            );
        });
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![wheel_event(ClientMouseKind::ScrollUp, 5)],
        }
    );
}

/// A passthrough combination that is also the link modifier (Cmd on macOS,
/// Ctrl elsewhere) still sends a right-click over a link to the application,
/// as in Herdr's TUI: a right-click opens no link.
#[gpui::test]
fn right_click_passthrough_wins_over_a_link_under_the_pointer(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:mouse");
    let link_modifier = gpui::Modifiers::secondary_key();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            let frame = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).frame;
            let url = "https://example.com/path";
            frame.cells = (0..frame.height)
                .flat_map(|y| (0..frame.width).map(move |x| (x, y)))
                .map(|(x, y)| CellData {
                    symbol: (y == 4)
                        .then(|| url.chars().nth(usize::from(x).wrapping_sub(1)))
                        .flatten()
                        .map_or_else(|| " ".into(), String::from),
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                })
                .collect();
            let position = mouse_position(view, 3.5, 4.5);
            assert!(view.link_modifier_held(position, MouseButton::Left, link_modifier));
            let right = MouseDownEvent {
                position,
                button: MouseButton::Right,
                modifiers: link_modifier,
                ..Default::default()
            };
            assert!(!view.terminal_mouse_down(&right, window, cx));
            let name = if cfg!(target_os = "macos") {
                "cmd"
            } else {
                "ctrl"
            };
            shared(
                view,
                &format!("[ui]\nright_click_passthrough_modifier = '{name}'"),
            );
            assert!(view.terminal_mouse_down(&right, window, cx));
        });
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![mouse_event(
                ClientMouseKind::Down(ClientMouseButton::Right),
                2,
                3
            )],
        }
    );
}

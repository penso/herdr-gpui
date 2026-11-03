//! A link-modifier click on a printed path in an application that reads the
//! mouse: the application keeps it until the path is verified.

use super::*;

#[gpui::test]
fn an_unchecked_path_leaves_the_click_to_the_application(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.txt"), "notes").unwrap();
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("local:mouse");
    let held = gpui::Modifiers::secondary_key();
    let position = cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            view.config.open_files_in = crate::config::FileTarget::System;
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.panes[0].cwd = Some(root.path().to_str().unwrap().into());
            snapshot.panes[0].foreground_cwd = snapshot.panes[0].cwd.clone();
            let frame = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).frame;
            let text = "notes.txt";
            frame.cells = (0..frame.height)
                .flat_map(|y| (0..frame.width).map(move |x| (x, y)))
                .map(|(x, y)| CellData {
                    symbol: (y == 4)
                        .then(|| text.chars().nth(usize::from(x).wrapping_sub(1)))
                        .flatten()
                        .map_or_else(|| " ".into(), String::from),
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                })
                .collect();
            mouse_position(view, 3.5, 4.5)
        })
    });
    let down = MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: held,
        click_count: 1,
        ..Default::default()
    };
    let up = MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: held,
        click_count: 1,
    };

    // Nothing has looked for the path yet, so the click is the application's.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.terminal_mouse_down(&down, window, cx));
            assert!(view.terminal_mouse_up(&up, cx));
        });
    });
    for kind in [
        ClientMouseKind::Down(ClientMouseButton::Left),
        ClientMouseKind::Up(ClientMouseButton::Left),
    ] {
        let ClientMessage::ClientShellPaneInput { pane_id, events } = server.receive() else {
            panic!("expected pane input");
        };
        assert_eq!(pane_id, "w1:p1");
        assert!(
            matches!(&events[..], [ClientPaneInputEvent::Mouse { kind: sent, .. }] if *sent == kind)
        );
    }
    assert!(cx.opened_url().is_none());

    // Once the path is found, the next click is the link's and opens it.
    cx.update(|_, cx| view.update(cx, |view, cx| view.hover_link(position, held, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.terminal_mouse_down(&down, window, cx));
            let pressed = view.terminal_link_press(position, held).unwrap();
            assert!(view.activate_terminal_link(&pressed, position, held, false, window, cx));
        });
    });
    cx.run_until_parked();
    let real = root.path().join("notes.txt").canonicalize().unwrap();
    let expected = url::Url::from_file_path(real).unwrap();
    assert_eq!(cx.opened_url().as_deref(), Some(expected.as_str()));
}

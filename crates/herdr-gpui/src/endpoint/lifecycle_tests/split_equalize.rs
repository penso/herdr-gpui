use super::*;

fn borders() -> Vec<PaneSurfaceSplit> {
    vec![
        PaneSurfaceSplit {
            direction: PaneSurfaceSplitDirection::Horizontal,
            pos: 40,
            area: SurfaceRect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            hit_rect: SurfaceRect {
                x: 40,
                y: 0,
                width: 1,
                height: 24,
            },
            path: vec![],
        },
        PaneSurfaceSplit {
            direction: PaneSurfaceSplitDirection::Vertical,
            pos: 12,
            area: SurfaceRect {
                x: 41,
                y: 0,
                width: 39,
                height: 24,
            },
            hit_rect: SurfaceRect {
                x: 41,
                y: 12,
                width: 39,
                height: 1,
            },
            path: vec![true],
        },
    ]
}

/// A window connected to `id` whose tab shows one pane beside a stack of two.
fn with_borders<'a>(
    cx: &'a mut gpui::TestAppContext,
    id: &str,
) -> (
    gpui::Entity<HerdrWindow>,
    Server,
    &'a mut gpui::VisualTestContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, server) = connected_endpoint(id);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            Arc::make_mut(view.live.surface.as_mut().unwrap()).splits = borders();
            let surface = view.live.surface.clone().unwrap();
            view.endpoints[1]
                .connection
                .inbox
                .lock()
                .unwrap()
                .apply(ClientEvent::Surface(surface));
        });
    });
    (view, server, cx)
}

fn down(
    view: &mut HerdrWindow,
    column: f32,
    row: f32,
    clicks: usize,
    cx: &mut Context<HerdrWindow>,
) -> bool {
    view.split_mouse_down(
        &MouseDownEvent {
            position: mouse_position(view, column, row),
            button: MouseButton::Left,
            click_count: clicks,
            ..Default::default()
        },
        cx,
    )
}

fn up(view: &mut HerdrWindow, column: f32, row: f32, cx: &mut Context<HerdrWindow>) {
    assert!(view.split_mouse_up(
        &MouseUpEvent {
            position: mouse_position(view, column, row),
            button: MouseButton::Left,
            ..Default::default()
        },
        cx
    ));
}

/// Lets go and checks that nothing is left to send.
fn release(view: &mut HerdrWindow, column: f32, row: f32, cx: &mut Context<HerdrWindow>) {
    up(view, column, row, cx);
    view.flush_split(cx);
    assert!(view.split_drag.is_none());
    assert!(view.live.drag_request.is_none());
}

fn set_zoomed(view: &mut HerdrWindow, zoomed: bool) {
    Arc::make_mut(view.live.snapshot.as_mut().unwrap())
        .tabs
        .iter_mut()
        .find(|tab| tab.tab_id == "w1:t1")
        .unwrap()
        .zoomed = zoomed;
}

/// The next split ratio request, answered.
fn ratio_of(server: &mut Server) -> (serde_json::Value, f64) {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing split ratio request");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "layout.set_split_ratio");
    assert_eq!(request["params"]["tab_id"], "w1:t1");
    let path = request["params"]["path"].clone();
    let ratio = request["params"]["ratio"].as_f64().unwrap();
    server.respond(&request);
    (path, ratio)
}

/// Answers each queued ratio. The next one is sent only after the previous
/// answer, which is the in-flight request the daemon allows.
fn evened(
    view: &gpui::Entity<HerdrWindow>,
    server: &mut Server,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(serde_json::Value, f64)> {
    let mut got = Vec::new();
    loop {
        got.push(ratio_of(server));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                project_until(view, cx, "split ratio answer", |view| {
                    view.live.drag_request.is_none()
                });
                view.flush_split(cx);
            });
        });
        let more = cx.update(|_, cx| view.read(cx).live.drag_request.is_some());
        if !more {
            break;
        }
    }
    got
}

fn assert_even(got: &[(serde_json::Value, f64)]) {
    assert_eq!(got.len(), borders().len());
    let ratio = |path: serde_json::Value| {
        got.iter()
            .find(|(got, _)| got == &path)
            .map(|(_, ratio)| *ratio)
    };
    let root = ratio(serde_json::json!([])).unwrap();
    let nested = ratio(serde_json::json!([true])).unwrap();
    // One pane beside a stack of two: the lone pane is one of three, and the
    // stack's border halves its two.
    assert!((root - 1.0 / 3.0).abs() < 1e-5, "root ratio was {root}");
    assert!((nested - 0.5).abs() < 1e-5, "nested ratio was {nested}");
}

#[gpui::test]
fn double_click_evens_every_split_and_a_drag_still_resizes(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = with_borders(cx, "ssh:even");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            // A click on the border is not a resize, and it is not terminal input.
            assert!(down(view, 40.5, 5., 1, cx));
            assert!(view.terminal_mouse.is_none());
            release(view, 40.5, 5., cx);

            // The nested border evens the whole tab, not only its own stack.
            assert!(down(view, 50., 12.5, 2, cx));
            assert!(view.terminal_mouse.is_none());
            assert!(view.live.drag_request.is_some());
            assert!(view.split_mouse_move(
                &MouseMoveEvent {
                    position: mouse_position(view, 20.5, 12.5),
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                cx
            ));
        });
    });
    assert_even(&evened(&view, &mut server, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| release(view, 20.5, 12.5, cx));
    });

    // The other border asks for the same splits, not only itself.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(down(view, 40.5, 5., 2, cx));
        });
    });
    assert_even(&evened(&view, &mut server, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| release(view, 40.5, 5., cx));
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            // A drag of the same border still follows the pointer.
            assert!(down(view, 40.5, 5., 1, cx));
            assert!(view.split_mouse_move(
                &MouseMoveEvent {
                    position: mouse_position(view, 20.5, 9.5),
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                cx
            ));
        });
    });
    let (path, dragged) = ratio_of(&mut server);
    assert_eq!(path, serde_json::json!([]));
    assert!((dragged - 0.25).abs() < 1e-5, "ratio was {dragged}");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            project_until(view, cx, "drag answer", |view| {
                view.live.drag_request.is_none()
            });
            release(view, 20.5, 9.5, cx);

            set_zoomed(view, true);
            assert!(down(view, 40.5, 5., 2, cx));
            assert!(view.split_drag.is_none());
            assert!(view.live.drag_request.is_none());
            set_zoomed(view, false);

            // Off the border, the pane still gets the click.
            assert!(!down(view, 3.5, 4.5, 1, cx));
            assert!(view.terminal_mouse_down(
                &MouseDownEvent {
                    position: mouse_position(view, 3.5, 4.5),
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx
            ));
            assert!(view.terminal_mouse.is_some());
        });
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![mouse_event(
                ClientMouseKind::Down(ClientMouseButton::Left),
                2,
                3
            )],
        }
    );
}

/// The usual double-click lets go before the first ratio is answered. The rest
/// still go out on the normal polling path, and the released drag retires
/// once every ratio is sent.
#[gpui::test]
fn an_early_release_lets_the_double_click_finish(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = with_borders(cx, "ssh:early");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(down(view, 50., 12.5, 2, cx));
            assert!(view.live.drag_request.is_some());
            up(view, 50., 12.5, cx);
            assert!(view.split_drag.is_some());
        });
    });
    assert_even(&evened(&view, &mut server, cx));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.split_drag.is_none());
        assert!(view.live.drag_request.is_none());
    });
}

/// A triple-click presses again while the double-click's ratios wait. The
/// press neither cuts the queue short nor picks up a border of its own.
#[gpui::test]
fn a_triple_click_lets_the_double_click_finish(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = with_borders(cx, "ssh:triple");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let off_border = mouse_position(view, 5., 5.);
            assert!(down(view, 50., 12.5, 2, cx));
            up(view, 50., 12.5, cx);
            assert_eq!(view.split_cursor_at(off_border), None);

            // The press is the border's, not the pane's, and picks up nothing.
            assert!(down(view, 50., 12.5, 3, cx));
            assert!(view.terminal_mouse.is_none());
            assert_eq!(view.split_cursor_at(off_border), None);
        });
    });
    assert_even(&evened(&view, &mut server, cx));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.split_drag.is_none());
        assert!(view.live.drag_request.is_none());
    });
}

/// Paths name splits within one tab, so a tab switch part-way through the
/// queue ends the drag rather than resizing the new tab's borders.
#[gpui::test]
fn a_tab_switch_drops_the_rest_of_an_even_queue(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = with_borders(cx, "ssh:switch");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(down(view, 50., 12.5, 2, cx));
        });
    });
    ratio_of(&mut server);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            project_until(view, cx, "split ratio answer", |view| {
                view.live.drag_request.is_none()
            });
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_tab_id =
                Some("w1:t2".into());
            view.flush_split(cx);
            assert!(view.split_drag.is_none());
            assert!(view.live.drag_request.is_none());

            // Back on the first tab, a drag's ratio is the next request on the
            // wire: the even ratio left in the queue was never sent.
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_tab_id =
                Some("w1:t1".into());
            assert!(down(view, 40.5, 5., 1, cx));
            assert!(view.split_mouse_move(
                &MouseMoveEvent {
                    position: mouse_position(view, 20.5, 9.5),
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                cx
            ));
        });
    });
    let (path, ratio) = ratio_of(&mut server);
    assert_eq!(path, serde_json::json!([]));
    assert!((ratio - 0.25).abs() < 1e-5, "ratio was {ratio}");
}

mod command;

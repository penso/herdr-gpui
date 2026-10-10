use super::*;
use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase};
use herdr_client::protocol::{
    PaneSurfaceScrollMetrics, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat,
    SurfaceGraphicsPlacement, SurfaceGraphicsSource, SurfaceGraphicsTarget,
};

/// A quarter row's trackpad delta follows the OS at once while the pane can
/// slide, but accumulates into whole lines while an image is on screen,
/// since images never slide: each delta must not become a line of its own.
#[gpui::test]
fn wheel_deltas_accumulate_into_lines_while_images_hold_the_grid(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for images in [false, true] {
        let (endpoint, mut server) = connected_endpoint("ssh:wheel");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint, cx);
                let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
                let pane = &mut surface.panes[0];
                pane.mouse_reporting = false;
                pane.scroll = Some(PaneSurfaceScrollMetrics {
                    offset_from_bottom: 0,
                    max_offset_from_bottom: 1000,
                    viewport_rows: 22,
                });
                if images {
                    surface.graphics.placements = vec![placement()];
                }
                view.presentation.frame(&view.live);
                let quarter = view.config.terminal.line_height() / 4.;
                view.scroll_wheel(
                    &ScrollWheelEvent {
                        position: mouse_position(view, 3.5, 4.5),
                        delta: ScrollDelta::Pixels(point(px(0.), px(quarter))),
                        touch_phase: TouchPhase::Moved,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
            });
        });
        let ClientMessage::ClientShellPaneInput { events, .. } = server.receive() else {
            panic!("missing input");
        };
        let sentinel = events == [ClientPaneInputEvent::TextCommit("sentinel".into())];
        assert_eq!(sentinel, images, "images={images}: {events:?}");
    }
}

fn placement() -> SurfaceGraphicsPlacement {
    SurfaceGraphicsPlacement {
        asset: SurfaceGraphicsAssetKey {
            source: SurfaceGraphicsSource::Terminal {
                target: SurfaceGraphicsTarget::Pane {
                    pane_id: "w1:p1".into(),
                },
                image_id: 1,
            },
            image_width: 1,
            image_height: 1,
            format: SurfaceGraphicsFormat::Rgba,
            data_len: 4,
            data_fingerprint: 1,
        },
        logical_placement_id: 1,
        x: 0,
        y: 0,
        cols: 1,
        rows: 1,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        x_offset: 0,
        y_offset: 0,
        z: -1,
        scrollback_offset: 0,
    }
}

/// A pane resting a quarter row into history shows, in the sliver at its
/// bottom edge, a row the live surface does not hold: no link resolves there,
/// while clicks and selection keep the edge row.
#[gpui::test]
fn the_uncovered_edge_of_a_resting_pane_resolves_no_link(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, _server) = connected_endpoint("ssh:wheel");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            let scroll = |offset| PaneSurfaceScrollMetrics {
                offset_from_bottom: offset,
                max_offset_from_bottom: 1000,
                viewport_rows: 22,
            };
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.panes[0].mouse_reporting = false;
            surface.panes[0].scroll = Some(scroll(0));
            // Rows that differ, so the daemon's next frame reads as a scroll.
            let frame = &mut surface.frame;
            frame.cells = (0..frame.height)
                .flat_map(|y| (0..frame.width).map(move |x| (x, y)))
                .map(|(x, y)| CellData {
                    symbol: if x == 2 { y.to_string() } else { " ".into() },
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                })
                .collect();
            view.presentation.frame(&view.live);
            let quarter = view.config.terminal.line_height() / 4.;
            view.scroll_wheel(
                &ScrollWheelEvent {
                    position: mouse_position(view, 3.5, 4.5),
                    delta: ScrollDelta::Pixels(point(px(0.), px(quarter))),
                    touch_phase: TouchPhase::Moved,
                    ..Default::default()
                },
                window,
                cx,
            );
            // The daemon shows the row the wheel asked for: its rows move down.
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.panes[0].scroll = Some(scroll(1));
            let width = usize::from(surface.frame.width);
            surface.frame.cells.rotate_right(width);
            view.presentation.frame(&view.live);
            // Hit testing follows what was painted: paint the pane at rest.
            let rest = Instant::now() + Duration::from_secs(1);
            assert!(view.presentation.scroll.slide(rest).is_some());
            let (middle, edge) = (
                mouse_position(view, 3.5, 2.5),
                mouse_position(view, 3.5, 22.9),
            );
            assert!(view.drawn_position(middle).is_some());
            assert_eq!(view.drawn_position(edge), None);
            let height = view.config.terminal.line_height();
            let (_, y) = view.grid_position(edge);
            assert_eq!((y / height).floor(), 22., "clicks keep the edge row");
        });
    });
}

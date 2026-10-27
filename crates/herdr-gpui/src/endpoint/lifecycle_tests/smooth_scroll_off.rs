use super::*;
use crate::sidebar::layout_tests::full_draw;
use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase};
use herdr_client::protocol::PaneSurfaceScrollMetrics;

/// Turning smooth scrolling off while a pane rests a quarter row into history
/// snaps it to the daemon's row on the next render: the slide is dropped and
/// clicks land on whole rows again.
#[gpui::test]
fn turning_smooth_scrolling_off_snaps_a_resting_pane_to_its_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let (endpoint, _server) = connected_endpoint("ssh:wheel");
    let height = view.update(cx, |view, cx| {
        prepare_mouse(view, endpoint, cx);
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.panes[0].mouse_reporting = false;
        surface.panes[0].scroll = Some(scroll_at(0));
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
        view.config.terminal.line_height()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let event = ScrollWheelEvent {
                position: mouse_position(view, 3.5, 4.5),
                delta: ScrollDelta::Pixels(point(px(0.), px(height / 4.))),
                touch_phase: TouchPhase::Moved,
                ..Default::default()
            };
            view.scroll_wheel(&event, window, cx);
            // The daemon shows the row the wheel asked for: its rows move down.
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.panes[0].scroll = Some(scroll_at(1));
            let width = usize::from(surface.frame.width);
            surface.frame.cells.rotate_right(width);
            view.presentation.frame(&view.live);
            let rest = Instant::now() + Duration::from_secs(1);
            assert!(view.presentation.scroll.slide(rest, false).is_some());
            let (_, y) = view.grid_position(mouse_position(view, 3.5, 2.5));
            assert_ne!(y / height, 2.5, "the resting pane is drawn between rows");
            view.config.smooth_scroll = false;
        });
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.update(cx, |view, _| {
        assert_eq!(view.presentation.scroll.offset(), None);
        let (_, y) = view.grid_position(mouse_position(view, 3.5, 2.5));
        assert_eq!(y / height, 2.5, "clicks land on whole rows");
    });
}

fn scroll_at(offset: usize) -> PaneSurfaceScrollMetrics {
    PaneSurfaceScrollMetrics {
        offset_from_bottom: offset as _,
        max_offset_from_bottom: 1000,
        viewport_rows: 22,
    }
}

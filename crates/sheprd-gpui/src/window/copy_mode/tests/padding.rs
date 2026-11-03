use super::*;
use crate::config::{Padding, Sides};
// `super::*` brings in gpui's `test`, which `#[gpui::test]` expands to.
use core::prelude::v1::test;
use gpui::px;

/// The badge marks the pane in copy mode, so it keeps its place in the
/// bottom right corner of the cells, not of the padding around them.
#[gpui::test]
fn copy_mode_badge_sits_over_the_cells_inside_the_terminal_padding(cx: &mut TestAppContext) {
    let peer = MockPeer::advertising(&["pane.copy_motion", "pane.selection.read", "pane.scroll"]);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        peer.prepare(&mut view, cx);
        view.live.supports_copy_motion = true;
        view.config.terminal_padding = Padding {
            x: Sides::new(16., 8.).unwrap(),
            y: Sides::new(4., 12.).unwrap(),
        };
        view
    });
    let redraw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    redraw(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::CopyMode, window, cx)));
    redraw(cx);

    let badge = cx.debug_bounds("copy-mode").unwrap();
    let terminal = cx.debug_bounds("terminal").unwrap();
    let (right, bottom) = view.read_with(cx, |view, _| {
        let rect = view.live.surface.as_ref().unwrap().panes[0].rect;
        assert_eq!(view.bounds.left(), terminal.left() + px(16.));
        assert_eq!(view.bounds.top(), terminal.top() + px(4.));
        (
            view.bounds.left() + px(f32::from(rect.x + rect.width) * view.cell_width),
            view.bounds.top()
                + px(f32::from(rect.y + rect.height) * view.config.terminal.line_height()),
        )
    });
    assert!((badge.right() - (right - px(6.))).abs() < px(0.5));
    assert!(badge.bottom() <= bottom);
    assert!(badge.top() >= bottom - px(32.5));
}

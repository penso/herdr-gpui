use super::*;
use crate::{
    config::{ClipboardToastPosition, Padding, Sides},
    terminal::viewport,
    window::Flash,
};
use gpui::{px, size};
use std::time::{Duration, Instant};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn terminal_padding_comes_out_of_all_four_sides_of_the_terminal(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(600.)));
    let set = |cx: &mut gpui::VisualTestContext, padding: Padding, sidebar: bool| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.config.terminal_padding = padding;
                view.sidebar_visible = sidebar;
                cx.notify();
            });
        });
    };

    // The shipped default puts the cells against every edge of the group.
    draw(cx);
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let outer = cx.debug_bounds("terminal").unwrap();
    let flush = view.read_with(cx, |view, _| view.bounds);
    assert_eq!(flush, outer);
    assert_eq!(flush.origin.x, sidebar.right());

    let padding = Padding {
        x: Sides::new(16., 8.).unwrap(),
        y: Sides::new(4., 12.).unwrap(),
    };
    set(cx, padding, true);
    draw(cx);
    // Padding moves nothing around the terminal.
    assert_eq!(cx.debug_bounds("sidebar").unwrap(), sidebar);
    assert_eq!(cx.debug_bounds("terminal").unwrap(), outer);
    // The same bounds feed painting, hit testing, and the resize the daemon
    // sees, so the padding must come out of the terminal's own size.
    let padded = view.read_with(cx, |view, _| view.bounds);
    assert_eq!(padded.origin.x, flush.origin.x + px(16.));
    assert_eq!(padded.origin.y, flush.origin.y + px(4.));
    assert_eq!(padded.size.width, flush.size.width - px(16. + 8.));
    assert_eq!(padded.size.height, flush.size.height - px(4. + 12.));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.options.surface_size,
            viewport(
                f32::from(padded.size.width),
                f32::from(padded.size.height),
                view.cell_width,
                view.config.terminal.line_height(),
            )
        );
    });

    // Unlike the gap it replaces, padding does not depend on the sidebar.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.settings.shared = Some(
                crate::herdr_settings::Settings::parse_text(
                    "[ui]\nsidebar_collapsed_mode = 'hidden'\n",
                )
                .unwrap(),
            );
        });
    });
    set(cx, padding, false);
    draw(cx);
    assert!(cx.debug_bounds("sidebar").is_none());
    let outer = cx.debug_bounds("terminal").unwrap();
    let hidden = view.read_with(cx, |view, _| view.bounds);
    assert_eq!(outer.origin.x, px(0.));
    assert_eq!(hidden.origin.x, px(16.));
    assert_eq!(hidden.origin.y, outer.origin.y + px(4.));
    assert_eq!(hidden.size.width, outer.size.width - px(16. + 8.));
    assert_eq!(hidden.size.height, outer.size.height - px(4. + 12.));
}

/// The flash reports on the cells, so a corner position is a corner of the
/// cells, not of the padding around them.
#[gpui::test]
fn clipboard_flash_sits_over_the_cells_inside_the_terminal_padding(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(600.)));
    let show = |cx: &mut gpui::VisualTestContext, position| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.config.terminal_padding = Padding {
                    x: Sides::new(16., 8.).unwrap(),
                    y: Sides::new(4., 12.).unwrap(),
                };
                view.config.clipboard_toast.position = position;
                view.flash = Some((
                    Flash::success("copied to clipboard"),
                    Instant::now() + Duration::from_secs(60),
                ));
                cx.notify();
            });
        });
        draw(cx);
        let terminal = cx.debug_bounds("terminal").unwrap();
        let cells = view.read_with(cx, |view, _| view.bounds);
        assert_eq!(cells.left(), terminal.left() + px(16.));
        assert_eq!(cells.right(), terminal.right() - px(8.));
        assert_eq!(cells.top(), terminal.top() + px(4.));
        assert_eq!(cells.bottom(), terminal.bottom() - px(12.));
        (cx.debug_bounds("flash").unwrap(), cells)
    };

    let (flash, cells) = show(cx, ClipboardToastPosition::TopLeft);
    assert_eq!(flash.left(), cells.left() + px(12.));
    assert_eq!(flash.top(), cells.top() + px(12.));

    let (flash, cells) = show(cx, ClipboardToastPosition::BottomRight);
    assert_eq!(flash.right(), cells.right() - px(12.));
    assert_eq!(flash.bottom(), cells.bottom() - px(12.));
}

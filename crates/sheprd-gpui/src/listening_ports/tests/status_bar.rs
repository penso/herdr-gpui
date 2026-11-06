use super::*;
use crate::sidebar::layout_tests::{fixture_window, full_draw};
use gpui::{TestAppContext, px, size};

#[gpui::test]
fn listening_ports_are_vertically_centered_in_the_status_bar(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.sidebar_visible = false;
        let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("w0".into());
        view.listening_ports.seed(
            Host::Local,
            parse("L 1 *:3000 node\nL 1 127.0.0.1:5173 vite\nE 1 w0\n").unwrap(),
        );
        view
    });
    for font_size in [10., 12., 18.] {
        view.update(cx, |view, cx| {
            view.config.ui.size = font_size;
            cx.notify();
        });
        for width in [480., 1000.] {
            cx.simulate_resize(size(px(width), px(600.)));
            cx.update(|window, cx| full_draw(window, cx).clear(cx));
            let bar = cx.debug_bounds("connection-status").unwrap();
            for selector in ["port-local-w0-3000", "port-local-w0-5173"] {
                let port = cx.debug_bounds(selector).unwrap();
                assert!(
                    (port.center().y - bar.center().y).abs() <= px(0.5),
                    "{selector} at font size {font_size}, width {width}: {port:?} in {bar:?}"
                );
            }
        }
    }
}

#![allow(clippy::unwrap_used)]
use crate::browser::WebUrl;
use gpui::{TestAppContext, VisualTestContext, px, size};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
}

/// The VS Code button appears once a server is set, just left of the
/// account slot, and only in builds that can show pages.
#[gpui::test]
fn the_vs_code_button_needs_a_server(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::titlebar::tests::header_window);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("toggle-code").is_none(), "no server set");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/").unwrap());
            cx.notify();
        })
    });
    draw(cx);
    let button = cx.debug_bounds("toggle-code");
    if !crate::browser::EMBEDDED {
        assert!(button.is_none(), "this build shows no pages");
        return;
    }
    let button = button.unwrap();
    let account = cx.debug_bounds("titlebar-account-slot").unwrap();
    assert_eq!(button.size, size(px(28.), px(28.)));
    assert!(button.right() <= account.left());
    assert!(account.left() - button.right() <= px(8.));
}

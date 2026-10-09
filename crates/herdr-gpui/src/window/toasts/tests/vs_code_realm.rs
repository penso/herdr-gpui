//! Toasts keep left of the VS Code column: its page draws above GPUI, so a
//! toast in the window's right corners would be hidden.
use super::*;
use crate::{controls::Command, sidebar::layout_tests::snapshot};
use gpui::{Bounds, Pixels};
use herdr_client::protocol::ToastHerdrPosition;
use std::sync::Arc;

/// The bounds of the VS Code column and of a top-right toast, in a window
/// `width` wide with the column showing.
fn corner_toast(cx: &mut TestAppContext, width: f32) -> (Bounds<Pixels>, Bounds<Pixels>) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    });
    cx.simulate_resize(size(px(width), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ToggleCode, window, cx);
            let mut wire = notification("Corner");
            wire.position = Some(ToastHerdrPosition::TopRight);
            view.endpoints[0]
                .toasts
                .receive([Notice::new(wire, Instant::now()).preview()]);
        });
        window.draw(cx).clear(cx);
    });
    (
        cx.debug_bounds("code").unwrap(),
        cx.debug_bounds("toast-local-0").unwrap(),
    )
}

#[gpui::test]
fn right_corner_toasts_sit_left_of_the_vs_code_column(cx: &mut TestAppContext) {
    let (code, toast) = corner_toast(cx, 1400.);
    assert_eq!(toast.right(), code.left() - px(12.));
    assert_eq!(toast.top(), px(72.));
}

/// A realm too narrow for a dialog gives dialogs the whole window, but no
/// menu makes the page step aside for a toast, so it still keeps left of it.
#[gpui::test]
fn toasts_keep_left_of_the_vs_code_column_in_a_narrow_window(cx: &mut TestAppContext) {
    let (code, toast) = corner_toast(cx, 700.);
    assert!(
        code.left() < px(480.),
        "the realm is too narrow for dialogs"
    );
    assert_eq!(toast.right(), code.left() - px(12.));
}

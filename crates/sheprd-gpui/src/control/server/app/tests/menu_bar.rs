//! `ui menubar` against a headless window.
use super::*;
use crate::sidebar::layout_tests::fixture_window;

#[gpui::test]
fn ui_menubar_switches_the_bar_or_says_the_system_owns_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let answer = cx.cx.update(|cx| {
        ui(
            call(UiOp::MenuBar {
                switch: Switch::Off,
            }),
            cx,
        )
    });
    let visible = view.read_with(cx, |view, _| view.menu_bar_visible);
    if crate::menus::IN_WINDOW {
        assert!(matches!(answer, Response::Done { .. }), "{answer:?}");
        assert!(!visible);
    } else {
        assert!(
            matches!(
                answer,
                Response::Error {
                    code: ErrorCode::Unsupported,
                    ..
                }
            ),
            "{answer:?}"
        );
        assert!(visible);
    }
}

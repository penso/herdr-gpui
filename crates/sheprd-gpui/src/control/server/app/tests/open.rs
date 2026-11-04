use super::*;

#[gpui::test]
fn open_needs_a_window(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let response = app(
            call(AppOp::Open {
                path: "/work/herdr".into(),
            }),
            cx,
        );
        assert_eq!(code(&response), Some(ErrorCode::NoWindow));
    });
}

#[gpui::test]
fn open_waits_for_the_local_herdr_and_leaves_no_palette_behind(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let response = open_in(view, "/work/herdr".into(), window, cx);
            // The fixture has no local Herdr behind it: the caller may try
            // again, and the window is left as it was.
            assert_eq!(code(&response), Some(ErrorCode::Busy));
            assert_eq!(view.menu.page, None);
            assert!(view.menu.palette.is_none());
        });
    });
}

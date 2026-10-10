use super::*;

#[gpui::test]
fn page_notes_wait_for_busy_delivery_and_feedback_before_resend(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_agent(&view, cx, "working");
    let tab = noted_tab(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.send_notes(&tab, cx);
            view.send_notes(&tab, cx);
            assert_eq!(view.deliveries.len(), 1);
        });
    });
    with_agent(&view, cx, "idle");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.poll_deliveries(cx);
            if cfg!(unix) {
                view.send_notes(&tab, cx);
                assert_eq!(view.deliveries.len(), 0, "feedback still owns the batch");
            }
        });
    });
    assert_eq!(
        kept(&view, cx)
            .unwrap()
            .matches("Note: Make it blue")
            .count(),
        1
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.send_notes(&tab, cx);
            assert_eq!(view.deliveries.len(), 1);
            view.poll_deliveries(cx);
        });
    });
    assert_eq!(
        kept(&view, cx)
            .unwrap()
            .matches("Note: Make it blue")
            .count(),
        1
    );
}

use super::*;

/// Page notes waiting for a busy agent can be taken back before it is idle,
/// and the next Send carries them again.
#[gpui::test]
fn page_notes_queued_for_a_busy_agent_can_be_withdrawn(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_agent(&view, cx, "working");
    let tab = noted_tab(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.send_notes(&tab, cx);
            assert_eq!(view.deliveries.len(), 1);
            assert_eq!(
                view.queued_notes(tab.id, cx),
                Some(crate::agent_notes::Queued::UntilIdle)
            );
            view.withdraw_notes(tab.id, cx);
            assert_eq!(view.deliveries.len(), 0);
            assert_eq!(view.queued_notes(tab.id, cx), None);
            assert_eq!(
                view.browser.annotations.listed(tab.id),
                [("Make it blue".to_owned(), false)]
            );
            view.send_notes(&tab, cx);
            assert_eq!(view.deliveries.len(), 1, "Send works again");
        });
    });
    with_agent(&view, cx, "idle");
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_deliveries(cx)));
    assert_eq!(
        kept(&view, cx)
            .unwrap()
            .matches("Note: Make it blue")
            .count(),
        1
    );
}

use super::*;
use herdr_client::protocol::AgentStatus;

fn set_status(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext, status: AgentStatus) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.agents[0].agent_status = status;
            view.live.snapshot = Some(Arc::new(shown));
        });
    });
}

fn sent(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) -> Vec<(String, bool)> {
    view.read_with(cx, |view, _| {
        view.reviews[&the(view)]
            .notes
            .iter()
            .map(|note| (note.comment.clone(), note.sent))
            .collect()
    })
}

fn shown(cx: &mut gpui::VisualTestContext, selector: &'static str) -> bool {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    cx.debug_bounds(selector).is_some()
}

/// A batch waiting for a busy agent shows as queued and can be taken back;
/// only its own notes become unsent, and Send carries them again.
#[gpui::test]
fn a_queued_batch_is_withdrawn_and_its_notes_sent_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, Some("working"));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));

    // A first round reaches the agent: with no connection in the fixture,
    // its paste fails over to `browser feedback`, which the agent fetches.
    note(&view, cx, line(3), "Delivered");
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_review(the(view), cx)));
    set_status(&view, cx, AgentStatus::Idle);
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_deliveries(cx)));
    assert!(kept(&view, cx).unwrap().contains("Note: Delivered"));

    set_status(&view, cx, AgentStatus::Working);
    note(&view, cx, RowId::Header(0), "Withdrawn");
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_review(the(view), cx)));
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 1));
    assert_eq!(
        sent(&view, cx),
        [("Delivered".into(), true), ("Withdrawn".into(), true)]
    );
    assert!(shown(cx, "review-queued") && shown(cx, "review-withdraw"));
    assert!(!shown(cx, "review-send"), "Send waits for the batch");

    cx.update(|_, cx| view.update(cx, |view, cx| view.withdraw_review(the(view), cx)));
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 0));
    assert_eq!(
        sent(&view, cx),
        [("Delivered".into(), true), ("Withdrawn".into(), false)]
    );
    assert!(!shown(cx, "review-queued") && shown(cx, "review-send"));
    // Nothing is left to take back.
    cx.update(|_, cx| view.update(cx, |view, cx| view.withdraw_review(the(view), cx)));
    assert_eq!(sent(&view, cx)[1], ("Withdrawn".into(), false));

    // Send carries the withdrawn note alone, and it still waits its turn.
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_review(the(view), cx)));
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 1));
    set_status(&view, cx, AgentStatus::Idle);
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_deliveries(cx)));
    let text = kept(&view, cx).unwrap();
    assert!(text.contains("Note: Withdrawn"), "{text}");
    assert!(!text.contains("Note: Delivered"), "{text}");
}

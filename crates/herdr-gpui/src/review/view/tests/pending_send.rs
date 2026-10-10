use super::*;

#[gpui::test]
fn pending_review_notes_are_sent_once_and_resend_after_collection(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, Some("working"));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    note(&view, cx, line(3), "Implement this once.");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.send_review(the(view), cx);
            view.send_review(the(view), cx);
            assert_eq!(view.deliveries.len(), 1);
        });
    });
    // An edit remains unsent while the original is still pending.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.edit_review_note(the(view), 0, window, cx);
            let input = view.reviews[&the(view)].input.clone();
            input.update(cx, |input, cx| input.set_text_selected("Edited once.", cx));
            view.add_review_note(the(view), window, cx);
            view.send_review(the(view), cx);
            assert!(!view.reviews[&the(view)].notes[0].sent);
            assert_eq!(view.deliveries.len(), 1);
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.agents[0].agent_status = herdr_client::protocol::AgentStatus::Idle;
            view.live.snapshot = Some(Arc::new(shown));
            view.poll_deliveries(cx);
            assert_eq!(view.deliveries.len(), 0);
            // Failed paste hands the pending batch to feedback, still once.
            if cfg!(unix) {
                view.send_review(the(view), cx);
                assert_eq!(view.deliveries.len(), 0);
            }
        });
    });
    let original = kept(cx).unwrap();
    assert_eq!(original.matches("Note: Implement this once.").count(), 1);
    assert!(!original.contains("Edited once."));
    for _ in 0..2 {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.send_review(the(view), cx);
                assert_eq!(view.deliveries.len(), 1);
                view.poll_deliveries(cx);
            });
        });
        let sent = kept(cx).unwrap();
        assert_eq!(sent.matches("Note: Edited once.").count(), 1);
    }
}

use super::*;

#[cfg(windows)]
#[gpui::test]
fn unavailable_feedback_copies_notes_and_releases_the_send(cx: &mut gpui::TestAppContext) {
    let mut sending = PendingSend::default();
    cx.update(|cx| {
        keep_or_copy(
            crate::browser::Batch {
                pane_id: "p1".into(),
                text: "Recoverable notes".into(),
            },
            sending.start(),
            "Could not reach the agent",
            cx,
        );
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("Recoverable notes")
        );
        assert!(!cx.default_global::<Feedback>().has("p1"));
        assert!(
            sending.start().is_some(),
            "the tab can send again without a control listener"
        );
    });
}

#[test]
fn feedback_holds_a_pending_send_until_taken_or_evicted() {
    let mut sending = PendingSend::default();
    let mut feedback = Feedback::default();
    let pending = sending.start();
    assert!(pending.is_some());
    feedback.keep(
        crate::browser::Batch {
            pane_id: "p1".into(),
            text: "one".into(),
        },
        pending,
    );
    assert!(sending.start().is_none());
    assert_eq!(feedback.take("p2"), None);
    assert!(sending.start().is_none());
    assert_eq!(feedback.take("p1").as_deref(), Some("one"));
    let pending = sending.start();
    assert!(pending.is_some());
    feedback.keep(
        crate::browser::Batch {
            pane_id: "p1".into(),
            text: "again".into(),
        },
        pending,
    );
    // The bounded feedback queue also releases evicted batches.
    for i in 0..32 {
        feedback.keep(
            crate::browser::Batch {
                pane_id: "p2".into(),
                text: i.to_string(),
            },
            None,
        );
    }
    assert!(sending.start().is_some());
}

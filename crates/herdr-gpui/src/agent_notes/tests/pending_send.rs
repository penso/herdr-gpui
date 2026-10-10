use super::*;

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

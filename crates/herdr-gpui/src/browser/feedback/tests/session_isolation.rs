use super::*;

#[test]
fn identical_pane_ids_in_different_sessions_never_share_notes_or_waiters() {
    let first = key("/first.sock", "p1");
    let second = key("/second.sock", "p1");
    let mut feedback = Feedback::default();
    feedback.set_waiting(vec![second.clone()]);
    assert_eq!(feedback.waiting(), std::slice::from_ref(&second));
    assert!(feedback.is_waiting(&second));
    assert!(!feedback.is_waiting(&first));
    feedback.keep(Batch {
        target: first.clone(),
        text: "first only".into(),
    });
    assert!(!feedback.has(&second));
    assert_eq!(feedback.take(&second), None);
    feedback.keep(Batch {
        target: second.clone(),
        text: "second only".into(),
    });
    assert_eq!(feedback.take(&first).as_deref(), Some("first only"));
    assert_eq!(feedback.take(&second).as_deref(), Some("second only"));
}

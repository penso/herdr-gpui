use super::*;

fn key(daemon: &str, pane: &str) -> FeedbackKey {
    FeedbackKey {
        scope: Scope::local(std::path::Path::new(daemon)),
        pane_id: pane.into(),
    }
}

fn batch(pane: &str, text: &str) -> Batch {
    Batch {
        target: key("/default.sock", pane),
        text: text.into(),
    }
}

#[test]
fn batches_are_taken_once_per_pane_and_bounded() {
    let mut feedback = Feedback::default();
    let p1 = key("/default.sock", "p1");
    let p2 = key("/default.sock", "p2");
    assert!(feedback.take(&p1).is_none());
    feedback.keep(batch("p1", "one"));
    feedback.keep(batch("p2", "other"));
    feedback.keep(batch("p1", "two"));
    assert!(feedback.has(&p1));
    assert_eq!(feedback.take(&p1).as_deref(), Some("one\ntwo"));
    assert!(feedback.take(&p1).is_none());
    assert_eq!(feedback.take(&p2).as_deref(), Some("other"));
    for index in 0..MAX_KEPT + 2 {
        feedback.keep(batch("p", &index.to_string()));
    }
    assert!(
        feedback
            .take(&key("/default.sock", "p"))
            .is_some_and(|text| text.starts_with("2\n"))
    );
}

mod session_isolation;

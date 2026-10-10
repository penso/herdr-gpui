use super::*;

#[test]
fn local_feedback_preserves_line_breaks_but_filters_other_controls() {
    let mut wire = notification("Local\nfeedback\u{202e}");
    wire.body = Some("Instructions\ncommand\r\t\u{202e}\u{0}".into());
    let local = Notice::local_feedback_multiline(wire.clone(), Instant::now());
    assert_eq!(local.title, "Localfeedback");
    assert_eq!(local.body.as_deref(), Some("Instructions\ncommand"));
    let single_line = Notice::local_feedback(wire.clone(), Instant::now());
    assert_eq!(single_line.body.as_deref(), Some("Instructionscommand"));
    let remote = Notice::new(wire, Instant::now());
    assert_eq!(remote.body.as_deref(), Some("Instructionscommand"));
}

#[test]
fn local_feedback_keeps_scan_bounds_and_rejects_blank_bodies() {
    let mut wire = notification("Local feedback");
    wire.body = Some(format!("{}end", "\n".repeat(512)));
    assert!(
        Notice::local_feedback_multiline(wire.clone(), Instant::now())
            .body
            .is_none()
    );
    wire.body = Some(format!("{}end", "a\n".repeat(256)));
    let local = Notice::local_feedback_multiline(wire, Instant::now());
    assert_eq!(local.body.as_deref(), Some("a\n".repeat(256).as_str()));
}

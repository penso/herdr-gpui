use super::*;

#[test]
fn the_apps_own_text_keeps_its_direction_marks() {
    let text = "\u{202e}Šéţţîñĝš \u{2068}key\u{2069}\u{202c}";
    let mut own = ConfigDiagnostic::default();
    own.sync_own(Some(text));
    assert_eq!(shown(&own), Some(vec![text.to_owned()]));

    // The daemon's text, which is untrusted, loses them.
    let mut daemon = ConfigDiagnostic::default();
    daemon.sync(Some(text));
    assert_eq!(shown(&daemon), Some(vec!["Šéţţîñĝš key".to_owned()]));
}

#[test]
fn the_apps_own_text_is_bounded() {
    let mut state = ConfigDiagnostic::default();
    let text = format!("{}\n2\n3\n4\n5", "y".repeat(500));
    state.sync_own(Some(&text));
    let lines = shown(&state).unwrap();
    assert_eq!(lines[0].chars().count(), MAX_LINE_CHARS);
    assert!(lines.len() <= MAX_LINES);
}

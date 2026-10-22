use super::*;

#[test]
fn a_card_opens_and_closes_on_the_text_it_shows() {
    let mut state = ConfigDiagnostic::default();
    state.sync(Some(
        "config-gpui.local.toml: layout.sidebar_gap is deprecated",
    ));
    assert!(!state.expanded());
    let lines = state.visible().unwrap().clone();
    assert!(state.toggle(&lines));
    assert!(state.expanded());
    // Snapshots repeating the text keep it open.
    state.sync(Some(
        "config-gpui.local.toml: layout.sidebar_gap is deprecated",
    ));
    assert!(state.expanded());
    assert!(state.toggle(&lines));
    assert!(!state.expanded());
}

#[test]
fn new_text_arrives_closed_and_a_stale_click_cannot_open_it() {
    let mut state = ConfigDiagnostic::default();
    state.sync(Some("first"));
    let first = state.visible().unwrap().clone();
    state.toggle(&first);
    state.sync(Some("second"));
    assert!(!state.expanded());
    assert!(!state.toggle(&first));
    assert!(!state.expanded());
    // Clearing and restoring the same text is a change too.
    let second = state.visible().unwrap().clone();
    state.toggle(&second);
    state.sync(None);
    state.sync(Some("second"));
    assert!(!state.expanded());
}

#[test]
fn a_dismissed_card_cannot_be_opened() {
    let mut state = ConfigDiagnostic::default();
    state.sync(Some("broken"));
    let lines = state.visible().unwrap().clone();
    state.dismiss(&lines);
    assert!(!state.toggle(&lines));
    assert!(!state.expanded());
}

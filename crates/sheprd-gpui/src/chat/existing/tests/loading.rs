use super::*;

fn pending() -> Existing {
    Existing {
        scope: Some(scope()),
        candidates: vec!["w1:p1".into()],
        due: Some(Instant::now() + SETTLED),
        ..Default::default()
    }
}

#[test]
fn first_discovery_loading_ends_on_empty_or_failed_answer_without_retry_flashes() {
    for result in [Ok(Vec::new()), Err(Error::Stopped)] {
        let mut state = pending();
        assert!(state.loading("w1:p1"));
        assert!(!state.loading("w9:p9"));
        state.probe = Some(answered(result));
        assert!(state.poll(Some(scope()), &["w1:p1".into()], 0, Instant::now()));
        assert!(!state.loading("w1:p1"));
        state.probe = Some(answered(Ok(Vec::new())));
        state.poll(Some(scope()), &["w1:p1".into()], 0, Instant::now());
        assert!(!state.loading("w1:p1"));
    }
}

#[test]
fn a_pending_terminal_choice_transfers_once_and_is_cleared_on_scope_change() {
    let mut state = pending();
    state.dismiss("w9:p9");
    assert!(state.loading("w1:p1"));
    assert!(!state.take_dismissed("w9:p9"));
    state.dismiss("w1:p1");
    assert!(!state.loading("w1:p1"));
    assert!(state.take_dismissed("w1:p1"));
    assert!(!state.take_dismissed("w1:p1"));
    state.dismiss("w1:p1");
    let mut next = scope();
    next.connection += 1;
    // No candidates: this reset cannot spawn any daemon/process lookup.
    state.poll(Some(next), &[], 0, Instant::now());
    assert!(!state.take_dismissed("w1:p1"));
    assert!(state.checked.is_empty());
    state.candidates.push("w1:p1".into());
    assert!(state.loading("w1:p1"));
}

//! Notifications tell what is new once, and never replay what the first
//! read found.
use super::*;
use crate::automations::notify::Seen;

#[test]
fn the_first_read_is_only_learned() {
    let mut seen = Seen::default();
    assert_eq!(seen.observe("box", &status()), []);
    assert_eq!(seen.observe("box", &status()), []);
}

#[test]
fn a_new_approval_or_failure_is_told_once() {
    let mut seen = Seen::default();
    seen.observe("box", &status());
    let mut next = status();
    let mut changed = script("lint", ScriptState::AwaitingApproval);
    changed.hash = "new".into();
    next.scripts.push(changed);
    next.runs
        .insert(0, run(129, "nightly", RunState::Halted, NOW));
    let notices = seen.observe("box", &next);
    let titles: Vec<String> = notices
        .iter()
        .map(|notice| notice.title.replace(['\u{2068}', '\u{2069}'], ""))
        .collect();
    assert_eq!(
        titles,
        ["nightly failed on box", "lint waits for approval on box"]
    );
    assert_eq!(seen.observe("box", &next), []);
    // The same file changed again needs approval again.
    if let Some(lint) = next.scripts.iter_mut().find(|script| script.name == "lint") {
        lint.hash = "newer".into();
    }
    assert_eq!(seen.observe("box", &next).len(), 1);
}

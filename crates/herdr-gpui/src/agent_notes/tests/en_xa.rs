//! The `en-XA` leak test for the notes text other views draw: the Send
//! button, the line under waiting notes, and the reasons notes were kept.
use super::*;
use crate::i18n::leak_tests::under_en_xa;
use herdr_i18n::pseudo::is_marked;

#[test]
fn the_send_button_and_queued_lines_are_messages() {
    under_en_xa("agent notes en-XA leak", |_, _| {
        for label in [send_label(2, 0), send_label(2, 2), send_label(3, 1)] {
            assert!(is_marked(&label), "not a message: {label}");
        }
        for queued in [Queued::UntilIdle, Queued::ForFeedback] {
            assert!(is_marked(&queued.label()), "not a message: {queued:?}");
        }
    });
}

#[test]
fn each_reason_notes_were_kept_has_its_own_sentence() {
    let kept = [
        Kept::NoAgent,
        Kept::NotReady,
        Kept::PaneGone,
        Kept::Unreachable,
    ]
    .map(|reason| crate::i18n::fl!("agent-notes-kept", reason = reason.selector()));
    assert_eq!(
        kept,
        [
            "No agent runs in that pane; notes kept for `browser feedback`",
            "The agent is not ready; notes kept for `browser feedback`",
            "The agent's pane is not here; notes kept for `browser feedback`",
            "Could not reach the agent; notes kept for `browser feedback`",
        ]
    );
}

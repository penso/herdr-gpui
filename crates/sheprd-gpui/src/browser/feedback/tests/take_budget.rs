#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn one_take_fits_one_answer_and_leaves_the_rest_kept() {
    let mut feedback = Feedback::default();
    let p1 = key("/default.sock", "p1");
    // Two fit in one take; the third waits for the next.
    let large = "x".repeat(TAKE_BUDGET / 3);
    for _ in 0..3 {
        feedback.keep(batch("p1", &large), None);
    }
    feedback.keep(batch("p2", "other"), None);
    let first = feedback.take(&p1).unwrap();
    assert!(json_size(&first) <= TAKE_BUDGET, "{}", first.len());
    assert_eq!(first.len(), 2 * large.len() + 1);
    assert!(
        feedback.has(&p1),
        "what did not fit stays for the next take"
    );
    assert_eq!(feedback.take(&p1).unwrap().len(), large.len());
    assert!(!feedback.has(&p1));
    assert_eq!(
        feedback.take(&key("/default.sock", "p2")).as_deref(),
        Some("other")
    );
}

#[test]
fn a_batch_alone_over_the_budget_is_still_taken() {
    let mut feedback = Feedback::default();
    let p1 = key("/default.sock", "p1");
    feedback.keep(batch("p1", &"y".repeat(TAKE_BUDGET + 1)), None);
    assert!(feedback.take(&p1).is_some(), "never kept forever");
    assert!(!feedback.has(&p1));
}

#[test]
fn a_batch_that_does_not_fit_holds_back_every_newer_one() {
    let mut feedback = Feedback::default();
    let p1 = key("/default.sock", "p1");
    let half = "a".repeat(TAKE_BUDGET / 2);
    feedback.keep(batch("p1", &half), None);
    feedback.keep(batch("p1", &"b".repeat(TAKE_BUDGET / 2 + 16)), None);
    feedback.keep(batch("p1", "c, a correction"), None);
    // The second does not fit after the first, so the third waits behind it.
    assert_eq!(feedback.take(&p1).unwrap(), half);
    // Then the second and the third together, still in order.
    let rest = feedback.take(&p1).unwrap();
    assert!(rest.starts_with('b') && rest.ends_with("\nc, a correction"));
    assert!(feedback.take(&p1).is_none());
}

#[test]
fn a_batch_kept_again_goes_first_and_an_oversized_one_is_cut_to_fit() {
    let mut feedback = Feedback::default();
    let p1 = key("/default.sock", "p1");
    feedback.keep(batch("p1", "newer"), None);
    feedback.keep_again(batch("p1", "older, never delivered"));
    assert_eq!(
        feedback.take(&p1).as_deref(),
        Some("older, never delivered\nnewer")
    );
    // Too long for one answer: cut when kept, so it is never stuck.
    feedback.keep(batch("p1", &"\u{1}".repeat(TAKE_BUDGET)), None);
    let taken = feedback.take(&p1).unwrap();
    assert!(json_size(&taken) <= TAKE_BUDGET, "{}", json_size(&taken));
    assert!(taken.ends_with("[Notes cut: too long for one answer]"));
}

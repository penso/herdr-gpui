use super::*;
use crate::apns::pacer::STEP_INTERVAL_MS;

fn live(pane: &str, event: Event, phase: Phase, step: &str) -> Live {
    Live {
        event,
        pane: pane.to_owned(),
        state: ContentState {
            cost: None,
            phase,
            plan_done: None,
            plan_total: None,
            question: None,
            started_at: 0,
            step: step.to_owned(),
            summary: None,
            waiting: 0,
            working: 0,
        },
    }
}

fn step(pane: &str, step: &str) -> Live {
    live(pane, Event::Update { alert: None }, Phase::Working, step)
}

#[test]
fn step_updates_are_held_back_keeping_the_newest() {
    let mut pacer = Pacer::default();
    assert!(pacer.offer(step("a", "one"), 0).is_some());
    assert!(pacer.offer(step("a", "two"), 1_000).is_none());
    assert!(pacer.offer(step("a", "three"), 2_000).is_none());
    assert!(pacer.offer(step("b", "other chat"), 2_000).is_some());
    assert!(pacer.due(STEP_INTERVAL_MS - 1).is_empty());
    let due = pacer.due(STEP_INTERVAL_MS);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].state.step, "three");
    assert!(pacer.due(STEP_INTERVAL_MS * 3).is_empty(), "sent once");
}

#[test]
fn questions_starts_and_ends_go_at_once() {
    let mut pacer = Pacer::default();
    assert!(pacer.offer(step("a", "one"), 0).is_some());
    let question = live("a", Event::Update { alert: None }, Phase::Question, "one");
    assert!(pacer.offer(question, 1).is_some());
    assert!(pacer.offer(step("a", "two"), 2).is_none());
    let end = live("a", Event::End { dismissal_s: 0 }, Phase::Finished, "two");
    assert!(pacer.offer(end, 3).is_some());
    assert!(
        pacer.due(STEP_INTERVAL_MS * 2).is_empty(),
        "an end drops the held step"
    );
}

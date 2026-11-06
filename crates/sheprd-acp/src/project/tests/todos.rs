use super::*;
use crate::project::todos::{MAX_PLAN, plan};

/// OpenCode 2.2's call to `todowrite`, as `opencode acp` sent it.
fn opencode(projector: &mut Projector, transcript: &mut Transcript, id: &str, todos: Value) {
    apply(
        projector,
        transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": id, "name": "todowrite",
            "title": "todowrite", "kind": "other", "status": "pending", "rawInput": {},
        }),
    );
    apply(
        projector,
        transcript,
        json!({
            "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "in_progress",
            "kind": "other", "title": "todowrite", "rawInput": {"todos": todos},
        }),
    );
    apply(
        projector,
        transcript,
        json!({
            "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "completed",
            "content": [{"type": "content", "content": {"type": "text", "text": "Todos updated"}}],
            "rawOutput": {"metadata": {"todos": []}},
        }),
    );
}

fn shown(transcript: &Transcript) -> Vec<(&str, PlanStatus)> {
    transcript
        .plan
        .iter()
        .map(|step| (step.content.as_str(), step.status))
        .collect()
}

#[test]
fn opencodes_todo_tool_shows_as_the_plan_and_not_as_a_card() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    opencode(
        &mut projector,
        &mut transcript,
        "call_1",
        json!([
            {"content": "alpha", "status": "in_progress"},
            {"content": "beta", "status": "pending"},
            {"content": "gamma", "status": "completed"},
            {"content": "dropped", "status": "cancelled"},
        ]),
    );
    assert_eq!(
        shown(&transcript),
        [
            ("alpha", PlanStatus::InProgress),
            ("beta", PlanStatus::Pending),
            ("gamma", PlanStatus::Completed),
        ]
    );
    assert!(transcript.entries.is_empty(), "no card for the call");
    assert!(projector.todos.is_empty(), "the ended call is forgotten");
}

#[test]
fn a_call_still_being_written_keeps_the_plan_and_an_empty_list_clears_it() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let list = json!([{"content": "alpha", "status": "pending"}]);
    opencode(&mut projector, &mut transcript, "call_1", list);
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": "call_2", "title": "todowrite",
            "status": "pending", "rawInput": {},
        }),
    );
    assert_eq!(shown(&transcript).len(), 1, "no list yet");
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call_update", "toolCallId": "call_2",
            "rawInput": {"todos": []},
        }),
    );
    assert!(transcript.plan.is_empty());
}

#[test]
fn copilots_checklist_shows_as_the_plan() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": "call_1", "title": "update_todo",
            "kind": "think", "status": "pending",
            "rawInput": {"todos": "## Steps\n- [x] Read\n  * [ ] Fix it\n1. [X] Test\n2) [ ] Ship\nnot a step\n- [ ]\n- [?] odd\n"},
        }),
    );
    assert_eq!(
        shown(&transcript),
        [
            ("Read", PlanStatus::Completed),
            ("Fix it", PlanStatus::Pending),
            ("Test", PlanStatus::Completed),
            ("Ship", PlanStatus::Pending),
        ]
    );
    assert!(transcript.entries.is_empty());
}

#[test]
fn a_real_plan_stands_over_the_tools_list() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "plan",
            "entries": [{"content": "real", "priority": "high", "status": "pending"}],
        }),
    );
    opencode(
        &mut projector,
        &mut transcript,
        "call_1",
        json!([{"content": "guessed", "status": "pending"}]),
    );
    assert_eq!(shown(&transcript), [("real", PlanStatus::Pending)]);
    assert!(transcript.entries.is_empty());
}

#[test]
fn a_sub_agents_list_and_other_tools_stay_cards() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": "call_1", "title": "Read todos.md",
            "status": "pending", "rawInput": {"todos": [{"content": "x", "status": "pending"}]},
        }),
    );
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": "call_2", "title": "todowrite",
            "status": "pending", "rawInput": {"todos": [{"content": "y", "status": "pending"}]},
            "_meta": Tag { activity: None, parent: Some("agent".to_owned()) }.meta(),
        }),
    );
    assert!(transcript.plan.is_empty());
    assert_eq!(transcript.entries.len(), 2);
}

#[test]
fn a_list_is_bounded() {
    let many: Vec<Value> = (0..MAX_PLAN + 10)
        .map(|index| json!({"content": index.to_string(), "status": "pending"}))
        .collect();
    let listed = plan(&json!({"todos": many})).unwrap();
    assert_eq!(listed.entries.len(), MAX_PLAN);
    let text = "- [ ] step\n".repeat(MAX_PLAN + 10);
    let checked = plan(&json!({"todos": text})).unwrap();
    assert_eq!(checked.entries.len(), MAX_PLAN);
}

#[test]
fn input_without_a_list_is_no_plan() {
    assert!(plan(&json!({})).is_none());
    assert!(plan(&json!({"todos": 3})).is_none());
}

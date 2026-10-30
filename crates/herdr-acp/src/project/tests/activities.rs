use herdr_chat_protocol::{ActivityKind, ActivityState, MAX_ACTIVITIES, MAX_ACTIVITY_TEXT};

use super::*;

fn tagged(projector: &mut Projector, transcript: &mut Transcript, value: Value, now: u64) {
    for change in projector.update(transcript, update(value), now) {
        transcript.apply(&change).unwrap();
    }
}

fn call(id: &str, title: &str, tag: &Tag) -> Value {
    json!({
        "sessionUpdate": "tool_call", "toolCallId": id, "title": title,
        "kind": "think", "status": "in_progress", "_meta": tag.meta(),
    })
}

fn ended(id: &str, status: &str, tag: &Tag) -> Value {
    json!({
        "sessionUpdate": "tool_call_update", "toolCallId": id,
        "status": status, "_meta": tag.meta(),
    })
}

fn subagent(state: ActivityState) -> Tag {
    Tag::activity(Report::new(ActivityKind::Subagent, state))
}

fn tool<'a>(transcript: &'a Transcript, call: &str) -> &'a Tool {
    transcript
        .entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) if tool.call_id == call => Some(tool),
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_subagent_runs_from_its_call_to_its_result_and_keeps_its_steps() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let running = subagent(ActivityState::Running);
    tagged(
        &mut projector,
        &mut transcript,
        call("task", "Explore the parser", &running),
        1_000,
    );
    assert_eq!(tool(&transcript, "task").kind, ToolKind::Subagent);
    let activity = transcript.activity("task").unwrap();
    assert_eq!(activity.label, "Explore the parser");
    assert_eq!(activity.kind, ActivityKind::Subagent);
    assert_eq!(activity.started_ms, 1_000);
    assert!(activity.is_running());

    let step = Tag {
        parent: Some("task".to_owned()),
        ..Tag::default()
    };
    tagged(
        &mut projector,
        &mut transcript,
        call("inner", "Read src/parser.rs", &step),
        1_500,
    );
    assert_eq!(tool(&transcript, "inner").parent.as_deref(), Some("task"));
    assert_eq!(
        transcript.activity("task").unwrap().detail.as_deref(),
        Some("Read src/parser.rs")
    );

    tagged(
        &mut projector,
        &mut transcript,
        ended("task", "completed", &subagent(ActivityState::Finished)),
        4_000,
    );
    let activity = transcript.activity("task").unwrap();
    assert_eq!(activity.state, ActivityState::Finished);
    assert_eq!(activity.finished_ms, Some(4_000));
    assert_eq!(activity.started_ms, 1_000);
    assert_eq!(
        tool(&transcript, "task").kind,
        ToolKind::Subagent,
        "an update without a kind keeps it"
    );
    // A late report does not start it again.
    tagged(
        &mut projector,
        &mut transcript,
        ended("task", "completed", &running),
        5_000,
    );
    assert_eq!(
        transcript.activity("task").unwrap().state,
        ActivityState::Finished
    );
}

#[test]
fn a_subagent_shows_a_bounded_number_of_steps() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    tagged(
        &mut projector,
        &mut transcript,
        call("task", "Search", &subagent(ActivityState::Running)),
        1_000,
    );
    let step = Tag {
        parent: Some("task".to_owned()),
        ..Tag::default()
    };
    for index in 0..MAX_STEPS + 5 {
        tagged(
            &mut projector,
            &mut transcript,
            call(&format!("step-{index}"), &format!("Step {index}"), &step),
            1_000,
        );
    }
    let steps = transcript
        .entries
        .iter()
        .filter(|entry| matches!(&entry.kind, EntryKind::Tool(tool) if tool.parent.is_some()))
        .count();
    assert_eq!(steps, MAX_STEPS);
    let last = format!("Step {}", MAX_STEPS + 4);
    assert_eq!(
        transcript.activity("task").unwrap().detail.as_deref(),
        Some(last.as_str()),
        "a step past the cap still names what the sub-agent does"
    );
}

#[test]
fn results_of_steps_past_the_cap_stay_inside_their_subagent() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    tagged(
        &mut projector,
        &mut transcript,
        call("task", "Search", &subagent(ActivityState::Running)),
        1_000,
    );
    let step = Tag {
        parent: Some("task".to_owned()),
        ..Tag::default()
    };
    let total = MAX_STEPS + 3;
    for index in 0..total {
        tagged(
            &mut projector,
            &mut transcript,
            call(&format!("step-{index}"), &format!("Step {index}"), &step),
            1_000,
        );
    }
    // Both bridges send a step's result without its sub-agent's tag.
    for index in 0..total {
        tagged(
            &mut projector,
            &mut transcript,
            ended(&format!("step-{index}"), "completed", &Tag::default()),
            2_000,
        );
    }
    let tools: Vec<&Tool> = transcript
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) => Some(tool),
            _ => None,
        })
        .collect();
    assert_eq!(
        tools.len(),
        MAX_STEPS + 1,
        "the sub-agent and its shown steps"
    );
    assert!(
        tools
            .iter()
            .all(|tool| tool.call_id == "task" || tool.parent.as_deref() == Some("task")),
        "no step's result comes back as a call of its own"
    );
    assert!(projector.skipped.is_empty(), "ended steps are forgotten");
}

#[test]
fn a_shell_label_and_detail_are_bounded_to_one_line() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let mut report = Report::new(ActivityKind::Shell, ActivityState::Running);
    report.label = Some(format!("{}\nsecond line", "é".repeat(MAX_ACTIVITY_TEXT)));
    tagged(
        &mut projector,
        &mut transcript,
        call("bash", "`npm run dev`", &Tag::activity(report)),
        1_000,
    );
    let label = &transcript.activity("bash").unwrap().label;
    assert!(label.len() <= MAX_ACTIVITY_TEXT);
    assert!(!label.contains('\n'));
    assert!(label.chars().all(|ch| ch == 'é'));
    // A call id too long to key an activity starts none.
    let long = "c".repeat(MAX_ACTIVITY_TEXT + 1);
    tagged(
        &mut projector,
        &mut transcript,
        call(&long, "x", &subagent(ActivityState::Running)),
        1_000,
    );
    assert!(transcript.activity(&long).is_none());
}

#[test]
fn a_malformed_tag_is_an_ordinary_call() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let value = json!({
        "sessionUpdate": "tool_call", "toolCallId": "c", "title": "Task", "kind": "think",
        "_meta": {"herdr": {"activity": {"kind": 7}, "parent": ["no"]}},
    });
    tagged(&mut projector, &mut transcript, value, 1_000);
    assert_eq!(tool(&transcript, "c").kind, ToolKind::Think);
    assert!(transcript.activities.is_empty());
}

#[test]
fn new_activities_wait_while_every_slot_still_runs() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let running = Tag::activity(Report::new(ActivityKind::Shell, ActivityState::Running));
    for index in 0..=MAX_ACTIVITIES {
        tagged(
            &mut projector,
            &mut transcript,
            call(&format!("shell-{index}"), "sleep", &running),
            1_000,
        );
    }
    assert_eq!(transcript.activities.len(), MAX_ACTIVITIES);
    assert_eq!(
        transcript.entries.len(),
        MAX_ACTIVITIES + 1,
        "the call shows"
    );
    let gone = kill_all(&transcript, 9_000);
    assert_eq!(gone.len(), MAX_ACTIVITIES);
    for change in gone {
        transcript.apply(&change).unwrap();
    }
    assert!(
        transcript
            .activities
            .iter()
            .all(|activity| activity.state == ActivityState::Killed
                && activity.finished_ms == Some(9_000))
    );
}

#[test]
fn an_ended_activity_of_a_dropped_call_does_not_bring_back_a_card() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let shell = |state| Tag::activity(Report::new(ActivityKind::Shell, state));
    tagged(
        &mut projector,
        &mut transcript,
        call("bash", "`npm run dev`", &shell(ActivityState::Running)),
        1_000,
    );
    // Later entries push the call out of the bounded transcript.
    tagged(
        &mut projector,
        &mut transcript,
        chunk("agent_message_chunk", "later", None),
        1_100,
    );
    let first = transcript.entries[1].id;
    transcript
        .apply(&Change::Truncate { before: first })
        .unwrap();
    projector.forget_before(first);
    let entries = transcript.entries.len();
    tagged(
        &mut projector,
        &mut transcript,
        ended("bash", "completed", &shell(ActivityState::Finished)),
        9_000,
    );
    assert_eq!(transcript.entries.len(), entries, "no bare card");
    let activity = transcript.activity("bash").unwrap();
    assert_eq!(activity.state, ActivityState::Finished);
    assert_eq!(activity.label, "`npm run dev`");
    // Heard of only once ended, an activity is not recorded at all.
    tagged(
        &mut projector,
        &mut transcript,
        call("late", "x", &shell(ActivityState::Finished)),
        9_000,
    );
    assert!(transcript.activity("late").is_none());
}

fn adapter(id: &str, update: &str, meta: Value, extra: Value) -> Value {
    let mut value = json!({
        "sessionUpdate": update, "toolCallId": id, "title": "Explore", "_meta": meta,
    });
    if let (Some(value), Value::Object(extra)) = (value.as_object_mut(), extra) {
        value.extend(extra);
    }
    value
}

#[test]
fn claude_agent_acp_meta_shows_foreground_subagents_and_their_steps() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    tagged(
        &mut projector,
        &mut transcript,
        adapter(
            "task",
            "tool_call",
            json!({"claudeCode": {"toolName": "Agent"}}),
            json!({"rawInput": {"description": "Explore", "prompt": "Look"}}),
        ),
        1_000,
    );
    assert!(transcript.activity("task").unwrap().is_running());
    assert_eq!(tool(&transcript, "task").kind, ToolKind::Subagent);
    tagged(
        &mut projector,
        &mut transcript,
        adapter(
            "inner",
            "tool_call",
            json!({"claudeCode": {"toolName": "Read", "parentToolUseId": "task"}}),
            json!({"title": "Read a.rs"}),
        ),
        1_200,
    );
    assert_eq!(tool(&transcript, "inner").parent.as_deref(), Some("task"));
    tagged(
        &mut projector,
        &mut transcript,
        adapter(
            "task",
            "tool_call_update",
            json!({"claudeCode": {"toolName": "Agent"}}),
            json!({"status": "completed"}),
        ),
        3_000,
    );
    assert_eq!(
        transcript.activity("task").unwrap().state,
        ActivityState::Finished
    );
}

#[test]
fn claude_agent_acp_background_work_stays_an_ordinary_card() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    tagged(
        &mut projector,
        &mut transcript,
        adapter(
            "task",
            "tool_call",
            json!({"claudeCode": {"toolName": "Agent"}}),
            json!({"rawInput": {"description": "Explore", "run_in_background": true}}),
        ),
        1_000,
    );
    tagged(
        &mut projector,
        &mut transcript,
        adapter(
            "task",
            "tool_call_update",
            json!({"claudeCode": {"toolName": "Agent", "toolResponse": {"isAsync": true}}}),
            json!({"status": "completed"}),
        ),
        1_100,
    );
    assert!(
        transcript.activities.is_empty(),
        "its end cannot be known on this path"
    );
}

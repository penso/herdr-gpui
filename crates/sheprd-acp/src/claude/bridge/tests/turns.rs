use super::*;

fn stream(event: Value) -> Value {
    json!({"type": "stream_event", "event": event, "parent_tool_use_id": null, "session_id": SESSION})
}

fn assistant(id: &str, content: Value) -> Value {
    json!({
        "type": "assistant",
        "message": {"id": id, "role": "assistant", "content": content},
        "parent_tool_use_id": null,
        "session_id": SESSION,
    })
}

fn tool_result(id: &str, content: Value, failed: bool) -> Value {
    json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": content, "is_error": failed},
        ]},
        "parent_tool_use_id": null,
        "session_id": SESSION,
    })
}

fn can_use_tool(request_id: &str, tool: &str, input: Value, suggestions: Value) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {
            "subtype": "can_use_tool",
            "tool_name": tool,
            "input": input,
            "permission_suggestions": suggestions,
            "tool_use_id": "toolu_1",
        },
    })
}

/// The permission request an out sends the session: its id and params.
fn permission_request(out: &Out) -> (i64, &Value) {
    match out {
        Out::Session(Inbound::Request {
            id: wire::RequestId::Number(id),
            method,
            params,
        }) if method == "session/request_permission" => (*id, params),
        other => panic!("expected a permission request, got {other:?}"),
    }
}

fn chosen(id: i64, option: &str) -> Inbound {
    Inbound::Response {
        id: wire::RequestId::Number(id),
        result: Ok(json!({"outcome": {"outcome": "selected", "optionId": option}})),
    }
}

#[test]
fn streamed_text_is_not_shown_again_by_the_whole_message() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "hi");
    bridge.on_harness(&stream(
        json!({"type": "message_start", "message": {"id": "msg_1"}}),
    ));
    let thought = bridge.on_harness(&stream(json!({
        "type": "content_block_delta", "index": 0,
        "delta": {"type": "thinking_delta", "thinking": "Hmm"},
    })));
    assert_eq!(update(&thought[0])["sessionUpdate"], "agent_thought_chunk");
    let text = bridge.on_harness(&stream(json!({
        "type": "content_block_delta", "index": 1,
        "delta": {"type": "text_delta", "text": "Hel"},
    })));
    let chunk = update(&text[0]);
    assert_eq!(chunk["sessionUpdate"], "agent_message_chunk");
    assert_eq!(chunk["content"]["text"], "Hel");
    assert_eq!(chunk["messageId"], "msg_1");
    let empty = bridge.on_harness(&stream(json!({
        "type": "content_block_delta", "index": 0,
        "delta": {"type": "thinking_delta", "thinking": ""},
    })));
    assert!(empty.is_empty(), "redacted thinking shows nothing");
    let whole = bridge.on_harness(&assistant(
        "msg_1",
        json!([{"type": "thinking", "thinking": "Hmm"}, {"type": "text", "text": "Hello"}]),
    ));
    assert!(whole.is_empty());
    // Without pieces, as when partial messages are off, the whole message shows.
    let unstreamed = bridge.on_harness(&assistant(
        "msg_2",
        json!([{"type": "text", "text": "Bye"}]),
    ));
    assert_eq!(update(&unstreamed[0])["content"]["text"], "Bye");
}

#[test]
fn a_tool_call_runs_through_to_its_result() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "run it");
    let outs = bridge.on_harness(&assistant(
        "msg_1",
        json!([{"type": "tool_use", "id": "toolu_1", "name": "Bash",
                "input": {"command": "cargo test\n--quiet", "description": "Run tests"}}]),
    ));
    let call = update(&outs[0]);
    assert_eq!(call["sessionUpdate"], "tool_call");
    assert_eq!(call["toolCallId"], "toolu_1");
    assert_eq!(call["title"], "`cargo test` (+1 more line)");
    assert_eq!(call["content"][0]["content"]["text"], "cargo test\n--quiet");
    assert_eq!(call["kind"], "execute");
    // Pending is ACP's default, so it goes unsaid.
    assert!(call.get("status").is_none_or(|status| status == "pending"));
    let outs = bridge.on_harness(&tool_result("toolu_1", json!("ok: 3 passed"), false));
    let done = update(&outs[0]);
    assert_eq!(done["sessionUpdate"], "tool_call_update");
    assert_eq!(done["status"], "completed");
    assert_eq!(done["content"][0]["content"]["text"], "ok: 3 passed");
    let failed = bridge.on_harness(&tool_result("toolu_1", json!("boom"), true));
    assert_eq!(update(&failed[0])["status"], "failed");
    assert!(
        bridge
            .on_harness(&tool_result("toolu_unknown", json!("x"), false))
            .is_empty(),
        "a result for no known call"
    );
}

#[test]
fn the_todo_list_becomes_the_plan() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "plan");
    let outs = bridge.on_harness(&assistant(
        "msg_1",
        json!([{"type": "tool_use", "id": "toolu_t", "name": "TodoWrite", "input": {"todos": [
            {"content": "Read the code", "status": "completed", "activeForm": "Reading"},
            {"content": "Fix it", "status": "in_progress", "activeForm": "Fixing"},
        ]}}]),
    ));
    let plan = update(&outs[0]);
    assert_eq!(plan["sessionUpdate"], "plan");
    assert_eq!(plan["entries"][0]["status"], "completed");
    assert_eq!(plan["entries"][1]["content"], "Fix it");
    assert!(
        bridge
            .on_harness(&tool_result("toolu_t", json!("updated"), false))
            .is_empty(),
        "its result is not a call"
    );
}

#[test]
fn a_subagents_messages_stay_inside_its_call() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "delegate");
    let mut inner = assistant("msg_9", json!([{"type": "text", "text": "inner"}]));
    inner["parent_tool_use_id"] = json!("toolu_task");
    assert!(bridge.on_harness(&inner).is_empty());
}

#[test]
fn a_permission_question_is_asked_and_its_answer_goes_back() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "write");
    let input = json!({"file_path": "/work/a.txt", "content": "hi"});
    let outs = bridge.on_harness(&can_use_tool(
        "ctl_1",
        "Write",
        input.clone(),
        json!([{"type": "setMode", "mode": "acceptEdits", "destination": "session"}]),
    ));
    let (asked, params) = permission_request(&outs[0]);
    assert_eq!(params["sessionId"], SESSION);
    assert_eq!(params["toolCall"]["toolCallId"], "toolu_1");
    assert_eq!(params["toolCall"]["title"], "Write /work/a.txt");
    let options: Vec<(&str, &str)> = params["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|option| {
            (
                option["optionId"].as_str().unwrap(),
                option["kind"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        options,
        [
            ("allow", "allow_once"),
            ("always", "allow_always"),
            ("reject", "reject_once")
        ]
    );
    assert_eq!(
        params["options"][1]["name"],
        "Yes, and accept edits this session"
    );
    let answer = bridge.on_session(chosen(asked, "always"));
    let control = claude(&answer[0]);
    assert_eq!(control["type"], "control_response");
    assert_eq!(control["response"]["request_id"], "ctl_1");
    let decision = &control["response"]["response"];
    assert_eq!(decision["behavior"], "allow");
    assert_eq!(decision["updatedInput"], input);
    assert_eq!(decision["updatedPermissions"][0]["mode"], "acceptEdits");
    assert!(
        bridge.on_session(chosen(asked, "always")).is_empty(),
        "answered once"
    );
}

#[test]
fn a_refused_or_dropped_question_denies_the_tool() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "run");
    let ask = |bridge: &mut Bridge, control: &str| {
        let outs = bridge.on_harness(&can_use_tool(
            control,
            "Bash",
            json!({"command": "rm x"}),
            json!([]),
        ));
        let (id, params) = permission_request(&outs[0]);
        assert_eq!(
            params["options"].as_array().unwrap().len(),
            2,
            "no always without suggestions"
        );
        id
    };
    let refused = ask(&mut bridge, "ctl_1");
    let answer = bridge.on_session(chosen(refused, "reject"));
    let decision = &claude(&answer[0])["response"]["response"];
    assert_eq!(decision["behavior"], "deny");
    assert!(decision.get("interrupt").is_none());
    let dropped = ask(&mut bridge, "ctl_2");
    let answer = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(dropped),
        result: Ok(json!({"outcome": {"outcome": "cancelled"}})),
    });
    let decision = &claude(&answer[0])["response"]["response"];
    assert_eq!(decision["behavior"], "deny");
    assert_eq!(decision["interrupt"], true);
}

#[test]
fn a_question_with_choices_is_answered_through_the_tools_input() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "ask me");
    let input = json!({"questions": [{
        "question": "Which database?",
        "header": "Database",
        "multiSelect": false,
        "options": [{"label": "Postgres"}, {"label": "SQLite"}],
    }]});
    let outs = bridge.on_harness(&can_use_tool("ctl_q", "AskUserQuestion", input, json!([])));
    let (id, params) = permission_request(&outs[0]);
    assert_eq!(params["toolCall"]["title"], "Which database?");
    let names: Vec<&str> = params["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|option| option["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Postgres", "SQLite", "Answer in the chat"]);
    let answer = bridge.on_session(chosen(id, "answer-1"));
    let decision = &claude(&answer[0])["response"]["response"];
    assert_eq!(decision["behavior"], "allow");
    assert_eq!(
        decision["updatedInput"]["answers"]["Which database?"],
        "SQLite"
    );
    assert_eq!(
        decision["updatedInput"]["questions"][0]["header"],
        "Database"
    );
}

#[test]
fn approving_a_plan_picks_the_mode_to_code_in() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "plan it");
    let outs = bridge.on_harness(&can_use_tool(
        "ctl_p",
        "ExitPlanMode",
        json!({"plan": "# Plan"}),
        json!([]),
    ));
    let (id, params) = permission_request(&outs[0]);
    assert_eq!(params["options"][0]["optionId"], "plan-accept-edits");
    let answer = bridge.on_session(chosen(id, "plan-default"));
    let decision = &claude(&answer[0])["response"]["response"];
    assert_eq!(decision["updatedPermissions"][0]["mode"], "default");
}

#[test]
fn a_finished_turn_answers_the_prompt_with_its_tokens_and_usage() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "hi");
    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    let usage = update(&outs[0]);
    assert_eq!(usage["sessionUpdate"], "usage_update");
    assert_eq!(usage["size"], 200_000);
    assert_eq!(usage["used"], 1082);
    assert_eq!(usage["cost"]["currency"], "USD");
    let answer = response(&outs[1], 2).as_ref().unwrap();
    assert_eq!(answer["stopReason"], "end_turn");
    let tokens = crate::project::tokens(answer).unwrap();
    assert_eq!(
        (tokens.input, tokens.cached, tokens.output),
        (106, 2000, 40)
    );
    assert_eq!(tokens.thought, 12);
    // A result with no turn waiting, such as a background task's, only
    // updates the usage.
    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    assert_eq!(outs.len(), 1);
}

#[test]
fn stop_reasons_and_failures_end_the_turn() {
    let cases = [
        (result("success", Some("max_tokens")), Ok("max_tokens")),
        (result("success", Some("refusal")), Ok("refusal")),
        (result("error_max_turns", None), Ok("max_turn_requests")),
        (result("error_during_execution", None), Err(())),
    ];
    for (message, expected) in cases {
        let mut bridge = opened();
        prompt(&mut bridge, 2, "go");
        let outs = bridge.on_harness(&message);
        let answer = response(outs.last().unwrap(), 2);
        match expected {
            Ok(stop) => assert_eq!(answer.as_ref().unwrap()["stopReason"], stop),
            Err(()) => assert!(answer.is_err()),
        }
    }
    // An API failure arrives as a success that is an error.
    let mut bridge = opened();
    prompt(&mut bridge, 2, "go");
    let mut failure = result("success", None);
    failure["is_error"] = json!(true);
    failure["result"] = json!("API Error: overloaded");
    let outs = bridge.on_harness(&failure);
    let error = response(outs.last().unwrap(), 2).as_ref().unwrap_err();
    assert_eq!(error.message, "API Error: overloaded");
}

#[test]
fn cancelling_interrupts_claude_and_ends_the_turn_as_cancelled() {
    let mut bridge = opened();
    assert!(
        bridge
            .on_session(Inbound::Notification {
                method: "session/cancel".to_owned(),
                params: json!({"sessionId": SESSION}),
            })
            .is_empty(),
        "nothing to cancel"
    );
    prompt(&mut bridge, 2, "count");
    let cancel = || Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({"sessionId": SESSION}),
    };
    let outs = bridge.on_session(cancel());
    let interrupt = claude(&outs[0]);
    assert_eq!(interrupt["request"]["subtype"], "interrupt");
    assert!(bridge.on_session(cancel()).is_empty(), "interrupted once");
    let id = interrupt["request_id"].as_str().unwrap();
    assert!(
        bridge
            .on_harness(&json!({"type": "control_response", "response": {
                "subtype": "success", "request_id": id, "response": {"still_queued": []},
            }}))
            .is_empty()
    );
    // What Claude sends for an interrupted turn.
    let outs = bridge.on_harness(&result("error_during_execution", None));
    let answer = response(outs.last().unwrap(), 2).as_ref().unwrap();
    assert_eq!(answer["stopReason"], "cancelled");
    // The next turn starts fresh.
    prompt(&mut bridge, 3, "again");
    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    assert_eq!(
        response(outs.last().unwrap(), 3).as_ref().unwrap()["stopReason"],
        "end_turn"
    );
}

#[test]
fn a_local_commands_output_is_a_notice() {
    let mut bridge = opened();
    let outs = bridge.on_harness(&json!({
        "type": "user",
        "message": {"role": "user", "content": "<local-command-stdout>Set model to haiku</local-command-stdout>"},
        "parent_tool_use_id": null,
    }));
    let notice = update(&outs[0]);
    assert_eq!(notice["sessionUpdate"], "notice");
    assert_eq!(notice["title"], "Set model to haiku");
    let plain = json!({"type": "user", "message": {"role": "user", "content": "[Request interrupted by user]"}});
    assert!(bridge.on_harness(&plain).is_empty());
}

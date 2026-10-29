#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};

use super::*;
use crate::harness::tests::{file_name, file_uri};

mod background;

const THREAD: &str = "01a160d5-4de7-7262-b413-0ce5180918ce";

fn harness(out: &Out) -> &Value {
    match out {
        Out::Harness(message) => message,
        Out::Session(inbound) => panic!("expected a message for Codex, got {inbound:?}"),
    }
}

fn update(out: &Out) -> &Value {
    match out {
        Out::Session(Inbound::Notification { method, params }) if method == "session/update" => {
            assert_eq!(params["sessionId"], THREAD);
            &params["update"]
        }
        other => panic!("expected a session update, got {other:?}"),
    }
}

fn response(out: &Out, id: i64) -> &Result<Value, wire::Error> {
    match out {
        Out::Session(Inbound::Response {
            id: wire::RequestId::Number(answered),
            result,
        }) if *answered == id => result,
        other => panic!("expected the answer to {id}, got {other:?}"),
    }
}

fn request(id: i64, method: &str, params: Value) -> Inbound {
    Inbound::Request {
        id: wire::RequestId::Number(id),
        method: method.to_owned(),
        params,
    }
}

fn answer(id: &Value, result: Value) -> Value {
    json!({"id": id, "result": result})
}

fn models() -> Value {
    json!({"data": [
        {"id": "gpt-6.1-sol", "displayName": "GPT-6.1 Sol", "description": "Fast", "hidden": false,
         "isDefault": true, "defaultReasoningEffort": "medium",
         "supportedReasoningEfforts": [{"reasoningEffort": "low"}, {"reasoningEffort": "medium"}, {"reasoningEffort": "high"}]},
        {"id": "gpt-6-luna", "displayName": "GPT-6 Luna", "hidden": false, "isDefault": false,
         "defaultReasoningEffort": "low", "supportedReasoningEfforts": [{"reasoningEffort": "low"}]},
        {"id": "secret", "displayName": "Hidden", "hidden": true, "isDefault": false, "supportedReasoningEfforts": []},
    ], "nextCursor": null})
}

/// A bridge with its thread open, the session's `session/new` answered.
fn opened() -> Bridge {
    let mut bridge = Bridge::default();
    let start = bridge.start();
    let initialize = harness(&start[0]);
    assert_eq!(initialize["method"], "initialize");
    assert!(
        initialize.get("jsonrpc").is_none(),
        "Codex leaves the version out"
    );
    assert!(
        bridge
            .on_session(request(
                1,
                "session/new",
                json!({"cwd": "/work", "mcpServers": []})
            ))
            .is_empty()
    );
    let outs = bridge.on_harness(&answer(&initialize["id"], json!({"userAgent": "codex"})));
    assert_eq!(harness(&outs[0]), &json!({"method": "initialized"}));
    let list = harness(&outs[1]);
    assert_eq!(list["method"], "model/list");
    let start = harness(&outs[2]);
    assert_eq!(start["method"], "thread/start");
    assert_eq!(start["params"]["cwd"], "/work");
    let (list, start) = (list["id"].clone(), start["id"].clone());
    // The thread may answer before the models; the session waits for both.
    let thread = json!({"thread": {"id": THREAD}, "model": "gpt-5.4", "reasoningEffort": null});
    assert!(bridge.on_harness(&answer(&start, thread)).is_empty());
    let outs = bridge.on_harness(&answer(&list, models()));
    let opened = response(&outs[0], 1).as_ref().unwrap();
    assert_eq!(opened["sessionId"], THREAD);
    bridge
}

fn prompt(bridge: &mut Bridge, id: i64) -> Value {
    let outs = bridge.on_session(request(
        id,
        "session/prompt",
        json!({"sessionId": THREAD, "prompt": [
            {"type": "text", "text": "Fix it"},
            {"type": "image", "mimeType": "image/png", "data": "iVBOR"},
            {"type": "resource_link", "uri": file_uri("a.rs"), "name": "a.rs"},
        ]}),
    ));
    let start = harness(&outs[0]).clone();
    bridge.on_harness(&answer(
        &start["id"],
        json!({"turn": {"id": "turn-1", "status": "inProgress"}}),
    ));
    start
}

fn notify(bridge: &mut Bridge, method: &str, params: Value) -> Vec<Out> {
    bridge.on_harness(&json!({"method": method, "params": params}))
}

#[test]
fn the_session_opens_with_codexs_presets_and_models() {
    let mut bridge = Bridge::default();
    bridge.start();
    let init = bridge.on_session(request(0, "initialize", json!({"protocolVersion": 1})));
    assert_eq!(
        response(&init[0], 0).as_ref().unwrap()["agentCapabilities"]["loadSession"],
        true
    );

    let bridge = opened();
    let modes = bridge.settings.modes();
    assert_eq!(modes.current_mode_id.0.as_ref(), "auto");
    let options = crate::project::options(Some(
        &serde_json::to_value(bridge.settings.options()).unwrap(),
    ));
    let model = options.iter().find(|option| option.id == "model").unwrap();
    match &model.kind {
        herdr_chat_protocol::OptionKind::Select { choices, current } => {
            // A configured model this account lacks gives way to the default.
            assert_eq!(current, "gpt-6.1-sol");
            assert_eq!(choices.len(), 2, "hidden models stay hidden");
        }
        other => panic!("{other:?}"),
    }
    assert!(options.iter().any(|option| option.id == "effort"));
}

#[test]
fn a_loaded_session_resumes_its_thread() {
    let mut bridge = Bridge::default();
    let start = bridge.start();
    let id = harness(&start[0])["id"].clone();
    bridge.on_session(request(
        1,
        "session/load",
        json!({"sessionId": THREAD, "cwd": "/work", "mcpServers": []}),
    ));
    let outs = bridge.on_harness(&answer(&id, json!({})));
    let resume = harness(&outs[2]);
    assert_eq!(resume["method"], "thread/resume");
    assert_eq!(resume["params"]["threadId"], THREAD);
}

#[test]
fn a_prompt_starts_a_turn_with_the_chosen_settings() {
    let mut bridge = opened();
    let effort = bridge.on_session(request(
        5,
        "session/set_config_option",
        json!({"sessionId": THREAD, "configId": "effort", "value": "high"}),
    ));
    assert!(response(&effort[0], 5).is_ok());
    let mode = bridge.on_session(request(
        6,
        "session/set_mode",
        json!({"sessionId": THREAD, "modeId": "read-only"}),
    ));
    assert!(response(&mode[0], 6).is_ok());
    let start = prompt(&mut bridge, 2);
    let params = &start["params"];
    assert_eq!(start["method"], "turn/start");
    assert_eq!(params["threadId"], THREAD);
    assert_eq!(params["model"], "gpt-6.1-sol");
    assert_eq!(params["effort"], "high");
    assert_eq!(params["approvalPolicy"], "on-request");
    assert_eq!(params["sandboxPolicy"]["type"], "readOnly");
    assert_eq!(
        params["input"][0],
        json!({"type": "text", "text": "Fix it", "text_elements": []})
    );
    assert_eq!(params["input"][1]["url"], "data:image/png;base64,iVBOR");
    assert_eq!(
        params["input"][2],
        json!({"type": "mention", "name": "a.rs", "path": file_name("a.rs")})
    );
    let unknown = bridge.on_session(request(
        7,
        "session/set_mode",
        json!({"sessionId": THREAD, "modeId": "yolo"}),
    ));
    assert!(response(&unknown[0], 7).is_err());
}

#[test]
fn a_turn_streams_its_answer_and_tools_then_ends_with_its_tokens() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let delta = notify(
        &mut bridge,
        "item/agentMessage/delta",
        json!({"threadId": THREAD, "itemId": "msg_1", "delta": "On it"}),
    );
    assert_eq!(update(&delta[0])["content"]["text"], "On it");
    assert_eq!(update(&delta[0])["messageId"], "msg_1");
    let whole = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": {"type": "agentMessage", "id": "msg_1", "text": "On it"}}),
    );
    assert!(whole.is_empty(), "already streamed");
    let unstreamed = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": {"type": "agentMessage", "id": "msg_2", "text": "Done"}}),
    );
    assert_eq!(update(&unstreamed[0])["content"]["text"], "Done");
    let thought = notify(
        &mut bridge,
        "item/reasoning/summaryTextDelta",
        json!({"threadId": THREAD, "itemId": "r1", "delta": "Thinking"}),
    );
    assert_eq!(update(&thought[0])["sessionUpdate"], "agent_thought_chunk");

    let command = json!({"type": "commandExecution", "id": "exec-1", "command": "cargo test", "status": "inProgress"});
    let started = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": THREAD, "item": command}),
    );
    let call = update(&started[0]);
    assert_eq!(call["sessionUpdate"], "tool_call");
    assert_eq!(call["title"], "`cargo test`");
    assert_eq!(call["kind"], "execute");
    let mut finished = command.clone();
    finished["status"] = json!("completed");
    finished["aggregatedOutput"] = json!("3 passed");
    let done = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": finished}),
    );
    let done = update(&done[0]);
    assert_eq!(done["status"], "completed");
    assert_eq!(done["content"][0]["content"]["text"], "3 passed");

    let change = json!({"type": "fileChange", "id": "patch-1", "status": "completed",
        "changes": [{"path": "/work/a.rs", "kind": {"type": "update"}, "diff": "-a\n+b"}]});
    let edited = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": THREAD, "item": change}),
    );
    assert_eq!(update(&edited[0])["title"], "Edit /work/a.rs");
    assert_eq!(update(&edited[0])["locations"][0]["path"], "/work/a.rs");

    let plan = notify(
        &mut bridge,
        "turn/plan/updated",
        json!({"threadId": THREAD, "plan": [
            {"step": "Read", "status": "completed"}, {"step": "Fix", "status": "inProgress"},
        ]}),
    );
    assert_eq!(update(&plan[0])["entries"][1]["status"], "in_progress");

    let usage = notify(
        &mut bridge,
        "thread/tokenUsage/updated",
        json!({"threadId": THREAD, "tokenUsage": {
            "last": {"totalTokens": 1200, "inputTokens": 1000, "cachedInputTokens": 400, "outputTokens": 200, "reasoningOutputTokens": 50},
            "total": {}, "modelContextWindow": 400_000,
        }}),
    );
    assert_eq!(update(&usage[0])["used"], 1200);
    assert_eq!(update(&usage[0])["size"], 400_000);

    let end = notify(
        &mut bridge,
        "turn/completed",
        json!({"threadId": THREAD, "turn": {"id": "turn-1", "status": "completed"}}),
    );
    let ended = response(&end[0], 2).as_ref().unwrap();
    assert_eq!(ended["stopReason"], "end_turn");
    let tokens = crate::project::tokens(ended).unwrap();
    assert_eq!(
        (tokens.input, tokens.cached, tokens.output, tokens.thought),
        (600, 400, 200, 50)
    );
}

#[test]
fn another_threads_events_are_skipped() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let other = notify(
        &mut bridge,
        "item/agentMessage/delta",
        json!({"threadId": "other", "itemId": "m", "delta": "x"}),
    );
    assert!(other.is_empty());
}

#[test]
fn cancelling_interrupts_the_turn() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let outs = bridge.on_session(Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({"sessionId": THREAD}),
    });
    let interrupt = harness(&outs[0]);
    assert_eq!(interrupt["method"], "turn/interrupt");
    assert_eq!(
        interrupt["params"],
        json!({"threadId": THREAD, "turnId": "turn-1"})
    );
    let end = notify(
        &mut bridge,
        "turn/completed",
        json!({"threadId": THREAD, "turn": {"id": "turn-1", "status": "interrupted"}}),
    );
    assert_eq!(
        response(&end[0], 2).as_ref().unwrap()["stopReason"],
        "cancelled"
    );
}

#[test]
fn a_cancel_before_the_turn_has_an_id_waits_for_it() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(
        2,
        "session/prompt",
        json!({"sessionId": THREAD, "prompt": [{"type": "text", "text": "go"}]}),
    ));
    let start = harness(&outs[0]).clone();
    let cancel = bridge.on_session(Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({}),
    });
    assert!(cancel.is_empty());
    let outs = bridge.on_harness(&answer(&start["id"], json!({"turn": {"id": "turn-9"}})));
    assert_eq!(harness(&outs[0])["params"]["turnId"], "turn-9");
}

#[test]
fn a_failed_turn_fails_the_prompt() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let end = notify(
        &mut bridge,
        "turn/completed",
        json!({"threadId": THREAD, "turn": {
            "id": "turn-1", "status": "failed", "error": {"message": "model not supported"},
        }}),
    );
    assert_eq!(
        response(&end[0], 2).as_ref().unwrap_err().message,
        "model not supported"
    );
}

#[test]
fn an_approval_is_asked_and_its_answer_goes_back() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let outs = bridge.on_harness(&json!({
        "id": 40,
        "method": "item/commandExecution/requestApproval",
        "params": {"threadId": THREAD, "turnId": "turn-1", "itemId": "exec-1", "command": "rm -rf build\nls"},
    }));
    let Out::Session(Inbound::Request {
        id: wire::RequestId::Number(asked),
        method,
        params,
    }) = &outs[0]
    else {
        panic!("{:?}", outs[0]);
    };
    assert_eq!(method, "session/request_permission");
    assert_eq!(params["toolCall"]["toolCallId"], "exec-1");
    assert_eq!(params["toolCall"]["title"], "`rm -rf build` (+1 more line)");
    assert_eq!(
        params["toolCall"]["content"][0]["content"]["text"],
        "rm -rf build\nls"
    );
    let kinds: Vec<&str> = params["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["allow_once", "allow_always", "reject_once"]);
    let reply = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(*asked),
        result: Ok(json!({"outcome": {"outcome": "selected", "optionId": "session"}})),
    });
    assert_eq!(
        harness(&reply[0]),
        &json!({"id": 40, "result": {"decision": "acceptForSession"}})
    );
}

#[test]
fn a_dropped_approval_cancels_and_unknown_requests_are_refused() {
    let mut bridge = opened();
    let outs = bridge.on_harness(&json!({
        "id": "req-7",
        "method": "item/fileChange/requestApproval",
        "params": {"threadId": THREAD, "turnId": "t", "itemId": "patch-1", "reason": "Write outside the workspace"},
    }));
    let Out::Session(Inbound::Request {
        id: wire::RequestId::Number(asked),
        params,
        ..
    }) = &outs[0]
    else {
        panic!("{:?}", outs[0]);
    };
    // Codex's reason never becomes the title.
    assert_eq!(
        params["toolCall"]["title"],
        "Codex asks to apply its changes"
    );
    assert!(
        params["toolCall"].get("content").is_none_or(Value::is_null),
        "the diff stays"
    );
    let reply = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(*asked),
        result: Ok(json!({"outcome": {"outcome": "cancelled"}})),
    });
    assert_eq!(
        harness(&reply[0]),
        &json!({"id": "req-7", "result": {"decision": "cancel"}})
    );
    let refused =
        bridge.on_harness(&json!({"id": 9, "method": "attestation/generate", "params": {}}));
    assert_eq!(harness(&refused[0])["error"]["code"], -32601);
}

#[test]
fn a_question_with_choices_is_answered_by_its_id() {
    let mut bridge = opened();
    let outs = bridge.on_harness(&json!({
        "id": 3,
        "method": "item/tool/requestUserInput",
        "params": {"threadId": THREAD, "turnId": "t", "itemId": "ask-1", "questions": [
            {"id": "db", "header": "Database", "question": "Which one?", "options": [{"label": "Postgres"}, {"label": "SQLite"}]},
        ]},
    }));
    let Out::Session(Inbound::Request {
        id: wire::RequestId::Number(asked),
        params,
        ..
    }) = &outs[0]
    else {
        panic!("{:?}", outs[0]);
    };
    assert_eq!(params["toolCall"]["title"], "Which one?");
    let reply = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(*asked),
        result: Ok(json!({"outcome": {"outcome": "selected", "optionId": "answer-1"}})),
    });
    assert_eq!(
        harness(&reply[0])["result"],
        json!({"answers": {"db": {"answers": ["SQLite"]}}})
    );
}

#[test]
fn warnings_and_final_errors_are_notices() {
    let mut bridge = opened();
    let warning = notify(
        &mut bridge,
        "warning",
        json!({"threadId": THREAD, "message": "Model metadata not found"}),
    );
    assert_eq!(update(&warning[0])["severity"], "warning");
    let retrying = notify(
        &mut bridge,
        "error",
        json!({"threadId": THREAD, "willRetry": true, "error": {"message": "x"}}),
    );
    assert!(retrying.is_empty());
    let failed = notify(
        &mut bridge,
        "error",
        json!({"threadId": THREAD, "willRetry": false, "error": {"message": "quota"}}),
    );
    assert_eq!(update(&failed[0])["title"], "quota");
}

#[test]
fn an_extra_access_request_shows_what_it_grants() {
    let mut bridge = opened();
    let outs = bridge.on_harness(&json!({
        "id": 11,
        "method": "item/permissions/requestApproval",
        "params": {"threadId": THREAD, "turnId": "t", "itemId": "perm-1", "cwd": "/work",
            "reason": "Just a quick read", "startedAtMs": 0, "environmentId": null,
            "permissions": {"network": {"enabled": true}, "fileSystem": {"writableRoots": ["/"]}}},
    }));
    let Out::Session(Inbound::Request {
        id: wire::RequestId::Number(asked),
        params,
        ..
    }) = &outs[0]
    else {
        panic!("{:?}", outs[0]);
    };
    let call = &params["toolCall"];
    assert_eq!(call["title"], "Codex asks for more access");
    let shown = call["content"][0]["content"]["text"].as_str().unwrap();
    assert!(shown.contains("Network: {\"enabled\":true}"), "{shown}");
    assert!(shown.contains("\"writableRoots\":[\"/\"]"), "{shown}");
    assert_eq!(
        call["content"][1]["content"]["text"],
        "Codex says: Just a quick read"
    );
    let reply = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(*asked),
        result: Ok(json!({"outcome": {"outcome": "selected", "optionId": "turn"}})),
    });
    assert_eq!(harness(&reply[0])["result"]["scope"], "turn");
    assert_eq!(
        harness(&reply[0])["result"]["permissions"]["network"]["enabled"],
        true
    );
}

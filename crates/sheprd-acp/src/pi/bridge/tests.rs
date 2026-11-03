#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};

use super::*;
use crate::harness::tests::{file_name, file_uri};

const SESSION: &str = "5f1c9a2e-0000-4000-8000-00000000000a";

fn harness(out: &Out) -> &Value {
    match out {
        Out::Harness(message) => message,
        Out::Session(inbound) => panic!("expected a message for pi, got {inbound:?}"),
    }
}

fn update(out: &Out) -> &Value {
    match out {
        Out::Session(Inbound::Notification { method, params }) if method == "session/update" => {
            assert_eq!(params["sessionId"], SESSION);
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

fn done(command: &Value, data: Value) -> Value {
    json!({"type": "response", "id": command["id"], "command": command["type"], "success": true, "data": data})
}

/// A bridge with its session open.
fn opened() -> Bridge {
    let mut bridge = Bridge::default();
    let start = bridge.start();
    let kinds: Vec<&str> = start
        .iter()
        .map(|out| harness(out)["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["get_state", "get_available_models", "get_commands"]);
    let (state, models, commands) = (
        harness(&start[0]).clone(),
        harness(&start[1]).clone(),
        harness(&start[2]).clone(),
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
    let held = bridge.on_harness(&done(
        &commands,
        json!({"commands": [{"name": "fix-tests", "description": "Fix them"}]}),
    ));
    assert!(held.is_empty(), "commands wait for the session");
    bridge.on_harness(&done(
        &state,
        json!({"sessionId": SESSION, "thinkingLevel": "medium", "model": {"provider": "anthropic", "id": "claude-sonnet-5-5"}}),
    ));
    let outs = bridge.on_harness(&done(&models, json!({"models": [
        {"provider": "anthropic", "id": "claude-sonnet-5-5", "name": "Claude Sonnet 5.5", "reasoning": true},
        {"provider": "openai", "id": "gpt-6", "name": "GPT-6", "reasoning": false},
    ]})));
    let opened = response(&outs[0], 1).as_ref().unwrap();
    assert_eq!(opened["sessionId"], SESSION);
    assert!(
        opened.get("modes").is_none_or(Value::is_null),
        "pi has no modes"
    );
    let options = crate::project::options(opened.get("configOptions"));
    assert_eq!(options.len(), 2, "model and thinking");
    assert_eq!(
        update(&outs[1])["availableCommands"][0]["name"],
        "fix-tests"
    );
    bridge
}

fn prompt(bridge: &mut Bridge, id: i64) -> Value {
    let outs = bridge.on_session(request(
        id,
        "session/prompt",
        json!({"sessionId": SESSION, "prompt": [
            {"type": "text", "text": "Look"},
            {"type": "image", "mimeType": "image/png", "data": "iVBOR"},
            {"type": "resource_link", "uri": file_uri("a.rs"), "name": "a.rs"},
        ]}),
    ));
    let command = harness(&outs[0]).clone();
    assert!(
        bridge.on_harness(&done(&command, Value::Null)).is_empty(),
        "accepted"
    );
    command
}

#[test]
fn a_prompt_carries_its_pictures_and_files() {
    let mut bridge = opened();
    let command = prompt(&mut bridge, 2);
    assert_eq!(command["type"], "prompt");
    assert_eq!(
        command["message"],
        format!("Look\n\n@{}", file_name("a.rs"))
    );
    assert_eq!(
        command["images"][0],
        json!({"type": "image", "data": "iVBOR", "mimeType": "image/png"})
    );
}

#[test]
fn a_refused_prompt_fails_at_once() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(
        2,
        "session/prompt",
        json!({"sessionId": SESSION, "prompt": [{"type": "text", "text": "x"}]}),
    ));
    let command = harness(&outs[0]);
    let refusal = json!({"type": "response", "id": command["id"], "command": "prompt", "success": false, "error": "No model"});
    let outs = bridge.on_harness(&refusal);
    assert_eq!(
        response(&outs[0], 2).as_ref().unwrap_err().message,
        "No model"
    );
}

#[test]
fn a_turn_streams_and_ends_from_how_pi_stopped() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    bridge.on_harness(&json!({"type": "message_start", "message": {"role": "assistant"}}));
    let text = bridge.on_harness(&json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "delta": "Hi"}}));
    assert_eq!(update(&text[0])["content"]["text"], "Hi");
    assert_eq!(update(&text[0])["messageId"], "pi-1");
    let thought = bridge.on_harness(&json!({"type": "message_update", "assistantMessageEvent": {"type": "thinking_delta", "delta": "Hmm"}}));
    assert_eq!(update(&thought[0])["sessionUpdate"], "agent_thought_chunk");

    let started = bridge.on_harness(
        &json!({"type": "tool_execution_start", "toolCallId": "call_1", "toolName": "edit",
        "args": {"path": "/work/a.rs", "edits": [{"oldText": "a", "newText": "b"}]}}),
    );
    let call = update(&started[0]);
    assert_eq!(call["title"], "Edit /work/a.rs");
    assert_eq!(call["content"][0]["oldText"], "a");
    let ended = bridge.on_harness(
        &json!({"type": "tool_execution_end", "toolCallId": "call_1", "toolName": "edit",
        "result": {"content": [{"type": "text", "text": "Edited"}]}, "isError": false}),
    );
    let ended = update(&ended[0]);
    assert_eq!(ended["status"], "completed");
    assert!(ended.get("content").is_none(), "the diff already shows");

    let outs = bridge.on_harness(&json!({"type": "agent_end", "messages": [
        {"role": "user", "content": "Look"},
        {"role": "assistant", "stopReason": "toolUse", "usage": {"input": 100, "output": 20, "cacheRead": 50, "cacheWrite": 10}},
        {"role": "assistant", "stopReason": "stop", "usage": {"input": 30, "output": 5, "cacheRead": 0, "cacheWrite": 0}},
    ]}));
    let answer = response(&outs[0], 2).as_ref().unwrap();
    assert_eq!(answer["stopReason"], "end_turn");
    let tokens = crate::project::tokens(answer).unwrap();
    assert_eq!((tokens.input, tokens.cached, tokens.output), (140, 50, 25));
    let stats = harness(&outs[1]).clone();
    assert_eq!(stats["type"], "get_session_stats");
    let usage = bridge.on_harness(&done(
        &stats,
        json!({"cost": 0.12, "contextUsage": {"tokens": 6000, "contextWindow": 200_000}}),
    ));
    assert_eq!(update(&usage[0])["used"], 6000);
    assert_eq!(update(&usage[0])["cost"]["amount"], 0.12);
}

#[test]
fn cancelling_aborts_and_the_turn_ends_cancelled() {
    let mut bridge = opened();
    let idle = bridge.on_session(Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({}),
    });
    assert!(idle.is_empty());
    prompt(&mut bridge, 2);
    let outs = bridge.on_session(Inbound::Notification {
        method: "session/cancel".to_owned(),
        params: json!({}),
    });
    assert_eq!(harness(&outs[0])["type"], "abort");
    let outs = bridge.on_harness(
        &json!({"type": "agent_end", "messages": [{"role": "assistant", "stopReason": "aborted"}]}),
    );
    assert_eq!(
        response(&outs[0], 2).as_ref().unwrap()["stopReason"],
        "cancelled"
    );
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let outs = bridge.on_harness(&json!({"type": "agent_end", "messages": [
        {"role": "assistant", "stopReason": "error", "errorMessage": "401 invalid key"},
    ]}));
    assert_eq!(
        response(&outs[0], 2).as_ref().unwrap_err().message,
        "401 invalid key"
    );
}

#[test]
fn the_model_and_thinking_change_through_pi() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(
        3,
        "session/set_config_option",
        json!({"sessionId": SESSION, "configId": "model", "value": "openai/gpt-6"}),
    ));
    let command = harness(&outs[0]).clone();
    assert_eq!(command["type"], "set_model");
    assert_eq!(
        (command["provider"].as_str(), command["modelId"].as_str()),
        (Some("openai"), Some("gpt-6"))
    );
    let outs = bridge.on_harness(&done(
        &command,
        json!({"provider": "openai", "id": "gpt-6"}),
    ));
    let options = &response(&outs[0], 3).as_ref().unwrap()["configOptions"];
    let options = crate::project::options(Some(options));
    assert_eq!(options.len(), 1, "GPT-6 does not reason here");
    let outs = bridge.on_session(request(
        4,
        "session/set_config_option",
        json!({"sessionId": SESSION, "configId": "thinking", "value": "ultra"}),
    ));
    assert!(response(&outs[0], 4).is_err());
}

#[test]
fn an_extensions_questions_become_permission_requests() {
    let mut bridge = opened();
    let outs = bridge.on_harness(
        &json!({"type": "extension_ui_request", "id": "ui-1", "method": "select",
        "title": "Allow dangerous command?", "options": ["Allow", "Block"]}),
    );
    let Out::Session(Inbound::Request {
        id: wire::RequestId::Number(asked),
        params,
        ..
    }) = &outs[0]
    else {
        panic!("{:?}", outs[0]);
    };
    assert_eq!(params["toolCall"]["title"], "Allow dangerous command?");
    let answer = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(*asked),
        result: Ok(json!({"outcome": {"outcome": "selected", "optionId": "option-1"}})),
    });
    assert_eq!(
        harness(&answer[0]),
        &json!({"type": "extension_ui_response", "id": "ui-1", "value": "Block"})
    );

    let outs = bridge.on_harness(&json!({"type": "extension_ui_request", "id": "ui-2", "method": "confirm", "title": "Clear?"}));
    let Out::Session(Inbound::Request {
        id: wire::RequestId::Number(asked),
        ..
    }) = &outs[0]
    else {
        panic!("{:?}", outs[0]);
    };
    let answer = bridge.on_session(Inbound::Response {
        id: wire::RequestId::Number(*asked),
        result: Ok(json!({"outcome": {"outcome": "cancelled"}})),
    });
    assert_eq!(harness(&answer[0])["cancelled"], true);

    let typed = bridge.on_harness(
        &json!({"type": "extension_ui_request", "id": "ui-3", "method": "input", "title": "Name?"}),
    );
    assert_eq!(
        harness(&typed[0]),
        &json!({"type": "extension_ui_response", "id": "ui-3", "cancelled": true})
    );
    let note = bridge.on_harness(&json!({"type": "extension_ui_request", "id": "ui-4", "method": "notify", "message": "Saved"}));
    assert_eq!(update(&note[0])["title"], "Saved");
}

#[test]
fn a_loaded_session_keeps_the_id_it_was_asked_for() {
    let mut bridge = Bridge::default();
    let start = bridge.start();
    bridge.on_session(request(
        1,
        "session/load",
        json!({"sessionId": SESSION, "cwd": "/work", "mcpServers": []}),
    ));
    bridge.on_harness(&done(
        harness(&start[0]),
        json!({"sessionId": "another-full-id"}),
    ));
    let outs = bridge.on_harness(&done(harness(&start[1]), json!({"models": []})));
    let opened = response(&outs[0], 1).as_ref().unwrap();
    assert_eq!(opened["sessionId"], SESSION);
    assert!(
        crate::project::options(opened.get("configOptions")).is_empty(),
        "no model to pick before a provider is set up"
    );
}

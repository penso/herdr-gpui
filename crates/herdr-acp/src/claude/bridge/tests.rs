#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};

use super::*;
use crate::harness::tests::{file_name, file_uri};

mod background;
mod settings;
mod turns;

const SESSION: &str = "c3a1e0d4-0000-4000-8000-000000000001";

/// Claude's `initialize` answer, as Claude Code 2.1 sends it, cut down.
fn handshake(request_id: &str) -> Value {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": {
                "commands": [
                    {"name": "review", "description": "Review the diff", "argumentHint": "[branch]"},
                    {"name": "compact", "description": "Compact the conversation", "argumentHint": ""},
                ],
                "models": [
                    {
                        "value": "default", "resolvedModel": "claude-opus-5-5",
                        "displayName": "Default (recommended)", "description": "Opus 5.5",
                        "supportedEffortLevels": ["low", "medium", "high"],
                    },
                    {
                        "value": "haiku", "resolvedModel": "claude-haiku-5-5",
                        "displayName": "Haiku", "description": "Haiku 5.5",
                    },
                ],
                "current_permission_mode": "default",
            },
        },
    })
}

/// A bridge past its handshake with an open session, and what it sent.
fn opened() -> Bridge {
    let mut bridge = Bridge::new(SESSION.to_owned());
    let start = bridge.start().remove(0);
    let id = claude(&start)["request_id"].as_str().unwrap().to_owned();
    bridge.on_harness(&handshake(&id));
    let outs = bridge.on_session(request(
        1,
        "session/new",
        json!({"cwd": "/work", "mcpServers": []}),
    ));
    assert!(matches!(
        &outs[..],
        [Out::Session(Inbound::Response { result: Ok(_), .. })]
    ));
    bridge
}

fn request(id: i64, method: &str, params: Value) -> Inbound {
    Inbound::Request {
        id: wire::RequestId::Number(id),
        method: method.to_owned(),
        params,
    }
}

fn prompt(bridge: &mut Bridge, id: i64, text: &str) -> Vec<Out> {
    bridge.on_session(request(
        id,
        "session/prompt",
        json!({"sessionId": SESSION, "prompt": [{"type": "text", "text": text}]}),
    ))
}

/// The message an out sends Claude.
fn claude(out: &Out) -> &Value {
    match out {
        Out::Harness(message) => message,
        Out::Session(inbound) => panic!("expected a message for Claude, got {inbound:?}"),
    }
}

/// The `session/update` an out sends the session.
fn update(out: &Out) -> &Value {
    match out {
        Out::Session(Inbound::Notification { method, params }) if method == "session/update" => {
            assert_eq!(params["sessionId"], SESSION);
            &params["update"]
        }
        other => panic!("expected a session update, got {other:?}"),
    }
}

/// The answer an out gives the session's request `id`.
fn response(out: &Out, id: i64) -> &Result<Value, wire::Error> {
    match out {
        Out::Session(Inbound::Response {
            id: wire::RequestId::Number(answered),
            result,
        }) if *answered == id => result,
        other => panic!("expected the answer to {id}, got {other:?}"),
    }
}

fn result(subtype: &str, stop_reason: Option<&str>) -> Value {
    json!({
        "type": "result",
        "subtype": subtype,
        "is_error": subtype != "success",
        "stop_reason": stop_reason,
        "session_id": SESSION,
        "total_cost_usd": 0.0125,
        "usage": {
            "input_tokens": 6,
            "cache_creation_input_tokens": 100,
            "cache_read_input_tokens": 2000,
            "output_tokens": 40,
            "output_tokens_details": {"thinking_tokens": 12},
            "iterations": [{
                "input_tokens": 2,
                "cache_creation_input_tokens": 50,
                "cache_read_input_tokens": 1000,
                "output_tokens": 30,
            }],
        },
        "modelUsage": {"claude-haiku-5-5": {"contextWindow": 200_000}},
    })
}

#[test]
fn a_session_opened_before_the_handshake_is_answered_by_it() {
    let mut bridge = Bridge::new(SESSION.to_owned());
    let start = bridge.start().remove(0);
    assert_eq!(claude(&start)["request"]["subtype"], "initialize");
    let init = bridge.on_session(request(0, "initialize", json!({"protocolVersion": 1})));
    let capabilities = &response(&init[0], 0).as_ref().unwrap()["agentCapabilities"];
    assert_eq!(capabilities["loadSession"], true);
    assert_eq!(capabilities["promptCapabilities"]["image"], true);
    assert!(
        bridge
            .on_session(request(
                1,
                "session/new",
                json!({"cwd": "/work", "mcpServers": []})
            ))
            .is_empty(),
        "waits for Claude"
    );
    let id = claude(&start)["request_id"].as_str().unwrap().to_owned();
    let outs = bridge.on_harness(&handshake(&id));
    let opened = response(&outs[0], 1).as_ref().unwrap();
    assert_eq!(opened["sessionId"], SESSION);
    assert_eq!(opened["modes"]["currentModeId"], "default");
    let options = crate::project::options(opened.get("configOptions"));
    assert_eq!(options.len(), 2, "model and effort");
    let commands = update(&outs[1]);
    assert_eq!(commands["sessionUpdate"], "available_commands_update");
    assert_eq!(
        commands["availableCommands"][0]["input"]["hint"],
        "[branch]"
    );
    assert!(commands["availableCommands"][1].get("input").is_none());
}

#[test]
fn a_failed_handshake_fails_the_session() {
    let mut bridge = Bridge::new(SESSION.to_owned());
    let start = bridge.start().remove(0);
    let id = claude(&start)["request_id"].as_str().unwrap().to_owned();
    bridge.on_harness(&json!({
        "type": "control_response",
        "response": {"subtype": "error", "request_id": id, "error": "not logged in"},
    }));
    let outs = bridge.on_session(request(
        1,
        "session/new",
        json!({"cwd": "/work", "mcpServers": []}),
    ));
    let error = response(&outs[0], 1).as_ref().unwrap_err();
    assert!(error.message.contains("not logged in"), "{}", error.message);
}

#[test]
fn a_prompt_goes_to_claude_as_a_user_message() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(
        2,
        "session/prompt",
        json!({"sessionId": SESSION, "prompt": [
            {"type": "text", "text": "Look at this"},
            {"type": "image", "mimeType": "image/png", "data": "iVBORw0K"},
            {"type": "resource_link", "uri": file_uri("src/main.rs"), "name": "main.rs"},
            {"type": "resource", "resource": {"uri": file_uri("notes.md"), "text": "- one"}},
        ]}),
    ));
    let message = claude(&outs[0]);
    assert_eq!(message["type"], "user");
    assert_eq!(message["session_id"], SESSION);
    let content = message["message"]["content"].as_array().unwrap();
    assert_eq!(content[0], json!({"type": "text", "text": "Look at this"}));
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert_eq!(content[1]["source"]["data"], "iVBORw0K");
    assert_eq!(content[2]["text"], format!("@{}", file_name("src/main.rs")));
    assert_eq!(
        content[3]["text"],
        format!("<file path=\"{}\">\n- one\n</file>", file_name("notes.md"))
    );
    let again = prompt(&mut bridge, 3, "again");
    assert!(response(&again[0], 3).is_err(), "one turn at a time");
}

#[test]
fn claude_requests_herdr_cannot_answer_are_refused() {
    let mut bridge = opened();
    let outs = bridge.on_harness(&json!({
        "type": "control_request",
        "request_id": "abc",
        "request": {"subtype": "hook_callback", "callback_id": "hook_0"},
    }));
    let refusal = claude(&outs[0]);
    assert_eq!(refusal["response"]["subtype"], "error");
    assert_eq!(refusal["response"]["request_id"], "abc");
}

#[test]
fn unknown_methods_and_messages_are_answered_or_skipped() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(9, "session/fork", json!({})));
    assert_eq!(
        response(&outs[0], 9).as_ref().unwrap_err().code,
        wire::Error::method_not_found().code
    );
    assert!(
        bridge
            .on_harness(&json!({"type": "rate_limit_event"}))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&json!({"type": "something_new"}))
            .is_empty()
    );
    assert!(bridge.on_harness(&json!("not an object")).is_empty());
}

use sheprd_chat_protocol::{OptionCategory, OptionKind};

use super::*;

fn answered(bridge: &mut Bridge, control: &Out, response: Value) -> Vec<Out> {
    let id = claude(control)["request_id"].as_str().unwrap().to_owned();
    bridge.on_harness(&json!({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": id, "response": response},
    }))
}

fn refused(bridge: &mut Bridge, control: &Out) -> Vec<Out> {
    let id = claude(control)["request_id"].as_str().unwrap().to_owned();
    bridge.on_harness(&json!({
        "type": "control_response",
        "response": {"subtype": "error", "request_id": id, "error": "no such mode"},
    }))
}

fn set_option(bridge: &mut Bridge, id: i64, option: &str, value: &str) -> Vec<Out> {
    bridge.on_session(request(
        id,
        "session/set_config_option",
        json!({"sessionId": SESSION, "configId": option, "value": value}),
    ))
}

/// The choices and current value of setting `id` in a `configOptions` list.
fn select(options: &Value, id: &str) -> (Vec<String>, String) {
    let option = crate::project::options(Some(options))
        .into_iter()
        .find(|option| option.id == id)
        .unwrap();
    match option.kind {
        OptionKind::Select { choices, current } => (
            choices.into_iter().map(|choice| choice.id).collect(),
            current,
        ),
        OptionKind::Toggle { .. } => panic!("{id} is a select"),
    }
}

#[test]
fn the_modes_are_claudes_permission_modes() {
    let mut bridge = Bridge::new(SESSION.to_owned());
    let start = bridge.start().remove(0);
    let id = claude(&start)["request_id"].as_str().unwrap().to_owned();
    let mut ready = handshake(&id);
    ready["response"]["response"]["current_permission_mode"] = json!("plan");
    bridge.on_harness(&ready);
    let outs = bridge.on_session(request(
        1,
        "session/new",
        json!({"cwd": "/work", "mcpServers": []}),
    ));
    let opened = response(&outs[0], 1).as_ref().unwrap();
    let modes: Vec<&str> = opened["modes"]["availableModes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mode| mode["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        modes,
        ["default", "acceptEdits", "plan", "bypassPermissions"]
    );
    assert_eq!(opened["modes"]["currentModeId"], "plan");
    let options = crate::project::options(opened.get("configOptions"));
    assert_eq!(options[0].category, OptionCategory::Model);
    assert_eq!(options[1].category, OptionCategory::ThoughtLevel);
}

#[test]
fn a_mode_change_waits_for_claude() {
    let mut bridge = opened();
    let outs = bridge.on_session(request(
        2,
        "session/set_mode",
        json!({"sessionId": SESSION, "modeId": "plan"}),
    ));
    let control = &outs[0];
    assert_eq!(
        claude(control)["request"],
        json!({"subtype": "set_permission_mode", "mode": "plan"})
    );
    let done = answered(&mut bridge, control, json!({"mode": "plan"}));
    assert!(response(&done[0], 2).is_ok());
    // Claude saying so again changes nothing.
    assert!(
        bridge
            .on_harness(&json!({"type": "system", "subtype": "status", "permissionMode": "plan"}))
            .is_empty()
    );
    let outs = bridge.on_session(request(
        3,
        "session/set_mode",
        json!({"sessionId": SESSION, "modeId": "acceptEdits"}),
    ));
    let failed = refused(&mut bridge, &outs[0]);
    assert_eq!(
        response(&failed[0], 3).as_ref().unwrap_err().message,
        "no such mode"
    );
}

#[test]
fn a_mode_claude_moves_to_by_itself_is_followed() {
    let mut bridge = opened();
    let outs = bridge.on_harness(&json!({
        "type": "system", "subtype": "status", "status": null, "permissionMode": "acceptEdits",
    }));
    let update = update(&outs[0]);
    assert_eq!(update["sessionUpdate"], "current_mode_update");
    assert_eq!(update["currentModeId"], "acceptEdits");
    assert!(
        bridge
            .on_harness(
                &json!({"type": "system", "subtype": "status", "permissionMode": "dontAsk"})
            )
            .is_empty(),
        "a mode this program does not offer"
    );
    assert!(
        bridge
            .on_harness(&json!({"type": "system", "subtype": "status", "status": "requesting"}))
            .is_empty()
    );
}

#[test]
fn the_model_and_effort_change_through_claude() {
    let mut bridge = opened();
    let outs = set_option(&mut bridge, 2, "effort", "high");
    let control = claude(&outs[0]);
    assert_eq!(control["request"]["subtype"], "apply_flag_settings");
    assert_eq!(control["request"]["settings"]["effortLevel"], "high");
    let done = answered(&mut bridge, &outs[0], Value::Null);
    let options = &response(&done[0], 2).as_ref().unwrap()["configOptions"];
    assert_eq!(select(options, "effort").1, "high");
    let outs = set_option(&mut bridge, 3, "effort", "default");
    assert_eq!(
        claude(&outs[0])["request"]["settings"]["effortLevel"],
        Value::Null
    );
    answered(&mut bridge, &outs[0], Value::Null);

    let outs = set_option(&mut bridge, 4, "model", "haiku");
    assert_eq!(
        claude(&outs[0])["request"],
        json!({"subtype": "set_model", "model": "haiku"})
    );
    let done = answered(&mut bridge, &outs[0], Value::Null);
    let options = &response(&done[0], 4).as_ref().unwrap()["configOptions"];
    let (models, current) = select(options, "model");
    assert_eq!(models, ["default", "haiku"]);
    assert_eq!(current, "haiku");
    assert!(
        crate::project::options(Some(options))
            .iter()
            .all(|option| option.id != "effort"),
        "Haiku takes no effort level"
    );
    let unknown = set_option(&mut bridge, 5, "effort", "high");
    assert!(response(&unknown[0], 5).is_err());
    let unknown = set_option(&mut bridge, 6, "model", "gpt");
    assert!(response(&unknown[0], 6).is_err());
}

#[test]
fn a_model_claude_switches_to_by_itself_is_followed() {
    let mut bridge = opened();
    let init = |model: &str| json!({"type": "system", "subtype": "init", "model": model, "permissionMode": "default"});
    assert!(
        bridge.on_harness(&init("claude-opus-5-5")).is_empty(),
        "the default already runs it"
    );
    let outs = bridge.on_harness(&init("claude-haiku-5-5"));
    let update = update(&outs[0]);
    assert_eq!(update["sessionUpdate"], "config_option_update");
    assert_eq!(select(&update["configOptions"], "model").1, "haiku");
    assert!(bridge.on_harness(&init("claude-unknown-1")).is_empty());
}

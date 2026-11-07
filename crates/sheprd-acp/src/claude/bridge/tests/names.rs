use sheprd_chat_protocol::OptionKind;

use super::*;

/// The names a session opens with: its modes, then each select's choices.
fn opened(description: Value) -> (Vec<String>, Vec<Vec<String>>) {
    let mut bridge = Bridge::new(SESSION.to_owned());
    let start = bridge.start().remove(0);
    let id = claude(&start)["request_id"].as_str().unwrap().to_owned();
    let mut ready = handshake(&id);
    ready["response"]["response"]["models"][0]["description"] = description;
    bridge.on_harness(&ready);
    let outs = bridge.on_session(request(
        1,
        "session/new",
        json!({"cwd": "/work", "mcpServers": []}),
    ));
    let opened = response(&outs[0], 1).as_ref().unwrap();
    let modes = opened["modes"]["availableModes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mode| mode["name"].as_str().unwrap().to_owned())
        .collect();
    let options = crate::project::options(opened.get("configOptions"))
        .into_iter()
        .map(|option| match option.kind {
            OptionKind::Select { choices, .. } => {
                choices.into_iter().map(|choice| choice.name).collect()
            }
            OptionKind::Toggle { .. } => panic!("{} is a select", option.id),
        })
        .collect();
    (modes, options)
}

#[test]
fn no_choice_is_called_default() {
    let (modes, options) = opened(json!("Opus 5.5 · Best for everyday, complex tasks"));
    assert_eq!(modes[0], "Ask First");
    assert_eq!(options[0][0], "Opus 5.5 (recommended)");
    assert_eq!(options[1][0], "Auto");
    for name in modes.iter().chain(options.iter().flatten()) {
        assert!(!name.contains("Default"), "{name}");
    }
}

#[test]
fn a_default_model_without_a_description_shows_its_id() {
    let (_, options) = opened(Value::Null);
    assert_eq!(options[0][0], "claude-opus-5-5 (recommended)");
}

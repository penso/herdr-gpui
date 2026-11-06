use super::*;
use sheprd_chat_protocol::{Choice, ConfigOption, OptionCategory, OptionKind};

fn select(id: &str, category: OptionCategory, names: &[&str]) -> ConfigOption {
    ConfigOption {
        id: id.into(),
        category,
        name: id.into(),
        description: None,
        kind: OptionKind::Select {
            choices: names
                .iter()
                .map(|name| Choice {
                    id: (*name).into(),
                    name: (*name).into(),
                    description: None,
                    group: None,
                })
                .collect(),
            current: names[0].into(),
        },
    }
}

/// The terminal layout drops each setting's name, so two settings both at
/// "Default" differ only by their marks.
#[gpui::test]
fn terminal_footer_marks_each_setting(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.options = vec![
        select(
            "model",
            OptionCategory::Model,
            &["Default", "Opus", "Haiku"],
        ),
        select(
            "effort",
            OptionCategory::ThoughtLevel,
            &["Default", "Low", "High"],
        ),
        select("mode", OptionCategory::Mode, &["Plan", "Build"]),
    ];
    let _stream = attach_with(
        &view,
        &program,
        ServerMessage::Snapshot {
            cursor: cursor(1, 0),
            transcript: Box::new(transcript),
        },
        cx,
    );
    switch(&view, ChatLayoutMode::Terminal, cx);
    poll(&view, &program, cx);
    for (setting, icon) in [
        ("chat-setting-model", "chat-setting-model-icon"),
        ("chat-setting-effort", "chat-setting-effort-icon"),
        ("chat-setting-mode", "chat-setting-mode-icon"),
    ] {
        let button = cx.debug_bounds(setting).unwrap();
        let mark = cx.debug_bounds(icon).unwrap();
        assert!(mark.left() >= button.left(), "{setting}");
        assert!(mark.right() <= button.right(), "{setting}");
    }
}

use super::*;
use gpui::{Modifiers, px};
use sheprd_chat_protocol::{Choice, ConfigOption, OptionCategory, OptionKind, Usage};

#[gpui::test]
fn terminal_footer_keeps_all_controls_on_one_line(cx: &mut TestAppContext) {
    for status in [Status::Idle, Status::Working] {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        let program = Program::new();
        let mut transcript = Transcript::new("OpenCode");
        transcript.status = status.clone();
        transcript.entries = vec![agent(1, "Looking.")];
        transcript.usage = Some(Usage {
            used: 129_500,
            size: 400_000,
            cost: None,
        });
        transcript.options = [
            (
                "model",
                OptionCategory::Model,
                "openaipro/GPT-5.6 Luna with a long model name",
            ),
            ("effort", OptionCategory::ThoughtLevel, "Default"),
            ("mode", OptionCategory::Mode, "Build"),
        ]
        .into_iter()
        .map(|(id, category, name)| ConfigOption {
            id: id.into(),
            category,
            name: id.into(),
            description: None,
            kind: OptionKind::Select {
                choices: vec![Choice {
                    id: "selected".into(),
                    name: name.into(),
                    description: None,
                    group: None,
                }],
                current: "selected".into(),
            },
        })
        .collect();
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
        let mut model_width = None;
        for width in [800, 640, 560, 320] {
            view.update(cx, |view, cx| {
                view.config.chat.max_width = width;
                cx.notify();
            });
            poll(&view, &program, cx);
            let footer = cx.debug_bounds("chat-settings").unwrap();
            let attach = cx.debug_bounds("chat-attach").unwrap();
            let usage = cx.debug_bounds("chat-usage").unwrap();
            let options = cx.debug_bounds("chat-settings-options").unwrap();
            let send = cx
                .debug_bounds(if status == Status::Working {
                    "chat-stop"
                } else {
                    "chat-send"
                })
                .unwrap();
            assert_eq!(send.top(), footer.top(), "{status:?}, width {width}");
            assert!(usage.center().y >= attach.top() && usage.center().y <= attach.bottom());
            assert_eq!(usage.center().y, send.center().y);
            assert!(usage.right() <= send.left());
            assert_eq!(send.right(), footer.right() - px(2.));
            assert_eq!(footer.size.height, send.size.height, "no third row");
            assert!(options.right() <= usage.left());
            for selector in [
                "chat-setting-model",
                "chat-setting-effort",
                "chat-setting-mode",
                "chat-auto-accept",
            ] {
                let control = cx.debug_bounds(selector).unwrap();
                assert_eq!(control.center().y, send.center().y, "{selector}");
                if width >= 560 {
                    assert!(control.left() >= options.left(), "{selector}: {width}");
                    assert!(
                        control.right() <= options.right(),
                        "{selector}: {width}, control {control:?}, options {options:?}"
                    );
                }
            }
            let model = cx.debug_bounds("chat-setting-model").unwrap();
            if width == 800 {
                model_width = Some(model.size.width);
            } else if width == 560 {
                assert!(
                    model.size.width < model_width.unwrap(),
                    "model yields width"
                );
            }
        }
        view.update(cx, |view, cx| {
            view.config.chat.max_width = 800;
            cx.notify();
        });
        poll(&view, &program, cx);
        let model = cx.debug_bounds("chat-setting-model").unwrap();
        cx.simulate_click(model.center(), Modifiers::default());
        poll(&view, &program, cx);
        assert!(cx.debug_bounds("chat-menu").is_some());
    }
}

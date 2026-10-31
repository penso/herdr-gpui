use super::*;
use crate::config::Theme;
use core::prelude::v1::test;

#[gpui::test]
fn expanded_tool_code_copies_the_full_decoded_and_original_sources(cx: &mut TestAppContext) {
    let command = "printf 'café 中文\\n'\n\tprintf 'second\\n'\nprintf 'third\\n'\nprintf 'fourth\\n'\nprintf 'fifth\\n'\nprintf 'last\\n'\n";
    let original = serde_json::json!({
        "cmd": command,
        "cwd": "/tmp/synthetic-tool-preview",
    })
    .to_string();
    for theme in ["Default", "Catppuccin Latte"] {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        view.update(cx, |view, cx| {
            view.theme = Theme::builtin(theme).unwrap();
            cx.notify();
        });
        let program = Program::new();
        let _stream = attach_with(
            &view,
            &program,
            snapshot(
                cursor(1, 0),
                vec![Entry {
                    id: EntryId(1),
                    kind: EntryKind::Tool(Tool {
                        parent: None,
                        call_id: "synthetic-tool-copy".into(),
                        content: vec![ToolContent::Text {
                            text: original.clone(),
                        }],
                        kind: ToolKind::Execute,
                        locations: Vec::new(),
                        status: ToolStatus::Completed,
                        title: "Tool exec".into(),
                    }),
                }],
            ),
            cx,
        );
        timeline::fit_conversation(&view, &program, cx);
        assert!(
            cx.debug_bounds("chat-tool-content-1-0").is_none(),
            "{theme}"
        );
        assert!(cx.debug_bounds("chat-copy-code").is_none(), "{theme}");
        assert!(cx.debug_bounds("chat-copy-original").is_none(), "{theme}");
        let toggle = cx.debug_bounds("chat-tool-toggle-1").unwrap();
        cx.simulate_click(toggle.center(), Modifiers::default());
        wait_until(|| {
            poll(&view, &program, cx);
            cx.debug_bounds("chat-code-line-5").is_some()
        });
        assert!(cx.debug_bounds("chat-tool-details-1").is_some(), "{theme}");

        let copy = cx.debug_bounds("chat-copy-code").unwrap();
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard())
                .and_then(|item| item.text())
                .as_deref(),
            Some(command),
            "{theme}: Copy code preserves original tabs/Unicode"
        );
        let copy = cx.debug_bounds("chat-copy-original").unwrap();
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard())
                .and_then(|item| item.text())
                .as_deref(),
            Some(original.as_str()),
            "{theme}: Copy original preserves all input fields and JSON bytes"
        );
        poll(&view, &program, cx);
        assert!(
            cx.debug_bounds("chat-tool-details-1").is_some(),
            "{theme}: copy controls do not toggle the card"
        );
        let toggle = cx.debug_bounds("chat-tool-toggle-1").unwrap();
        cx.simulate_click(toggle.center(), Modifiers::default());
        poll(&view, &program, cx);
        assert!(
            cx.debug_bounds("chat-tool-content-1-0").is_none(),
            "{theme}"
        );
        assert!(cx.debug_bounds("chat-code-line-0").is_none(), "{theme}");
    }
}

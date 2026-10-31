use super::*;
use crate::config::Theme;

#[gpui::test]
fn expanded_edits_show_copyable_diff_with_both_sides(cx: &mut TestAppContext) {
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
                        call_id: "synthetic-diff".into(),
                        title: "Update sample".into(),
                        kind: ToolKind::Edit,
                        status: ToolStatus::Completed,
                        locations: Vec::new(),
                        content: vec![ToolContent::Diff {
                            path: "example.rs".into(),
                            old: Some("\tlet α = 2;\n".into()),
                            new: "\tlet α = 3;\n".into(),
                        }],
                    }),
                }],
            ),
            cx,
        );
        timeline::fit_conversation(&view, &program, cx);
        assert!(cx.debug_bounds("chat-copy-diff").is_none());
        let header = cx.debug_bounds("chat-tool-1").unwrap();
        cx.simulate_click(header.center(), Modifiers::default());
        wait_until(|| {
            poll(&view, &program, cx);
            cx.debug_bounds("chat-copy-diff").is_some()
        });
        assert!(cx.debug_bounds("chat-code-line-4").is_some());
        assert!(cx.debug_bounds("chat-tool-details-1").is_some());
        let copy = cx.debug_bounds("chat-copy-diff").unwrap();
        cx.simulate_click(copy.center(), Modifiers::default());
        let text = cx
            .update(|_, cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .unwrap();
        assert!(
            text.contains("-\tlet α = 2;\n+\tlet α = 3;\n"),
            "{theme}: {text}"
        );
        assert!(text.contains("@@ Provided text @@"));
        let copy = cx.debug_bounds("chat-copy-original").unwrap();
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard())
                .and_then(|item| item.text())
                .as_deref(),
            Some("example.rs\nBefore:\n\tlet α = 2;\n\nAfter:\n\tlet α = 3;\n")
        );
    }
}

use super::*;
use crate::chat::prepared::{Body, Part, Prepared};
use gpui::{Modifiers, TestAppContext};
use herdr_client::ConnectTarget;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
        },
    }
}

#[test]
fn user_code_math_and_xml_are_prepared_without_agent_summary_classification() {
    let text = "Inline `value` and $x^2$.\n\n```python\nprint('hello')\n```\n\n$$y=x+1$$\n\n<system>Quoted instructions stay literal.</system>";
    let summary = "## Objective\nReview this.\n## Work State\nPending.\n## Next Move\nExplain it.";
    let mut transcript = Transcript::new("Codex");
    transcript.entries = vec![user(1, text), user(2, summary), agent(3, summary)];
    let mut prepared = Prepared::default();
    prepared.update(
        &transcript,
        &[(EntryId(1), 1), (EntryId(2), 1), (EntryId(3), 1)],
    );
    let body = prepared.body(EntryId(1)).unwrap();
    let Body::Message(parts) = &*body else {
        panic!("user message")
    };
    let blocks: Vec<_> = parts
        .iter()
        .flat_map(|part| match part {
            Part::Rich(blocks) => blocks.iter(),
            _ => panic!("ordinary Markdown stays native"),
        })
        .collect();
    assert!(blocks.iter().any(
        |block| matches!(block, rich::Block::Code { language, source, .. }
        if language == "python" && source.as_ref() == "print('hello')\n")
    ));
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block, rich::Block::Math { source, .. }
        if source.as_ref() == "y=x+1"))
    );
    let inline: Vec<_> = blocks
        .iter()
        .flat_map(|block| match block {
            rich::Block::Paragraph { content, .. } => content.as_slice(),
            _ => &[],
        })
        .collect();
    assert!(
        inline.iter().any(
            |part| matches!(part, rich::Inline::Math { source, .. } if source.as_ref() == "x^2")
        )
    );
    assert!(inline.iter().any(
        |part| matches!(part, rich::Inline::Text(text, style) if text == "value" && style.code)
    ));
    let literal = inline
        .iter()
        .filter_map(|part| match part {
            rich::Inline::Text(text, _) => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert!(literal.contains("<system>Quoted instructions stay literal.</system>"));
    assert!(matches!(
        &*prepared.body(EntryId(2)).unwrap(),
        Body::Message(_)
    ));
    assert!(matches!(
        &*prepared.body(EntryId(3)).unwrap(),
        Body::Summary(_)
    ));
}

#[gpui::test]
fn user_messages_render_native_code_math_and_copy_the_original_prompt(cx: &mut TestAppContext) {
    use crate::config::{ChatLayoutMode, Theme};
    for (layout, theme, mark) in [
        (ChatLayoutMode::Document, "Default", "chat-heading-1"),
        (ChatLayoutMode::IMessage, "Default", "chat-imessage-1"),
        (
            ChatLayoutMode::IMessage,
            "Catppuccin Latte",
            "chat-imessage-1",
        ),
    ] {
        let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
        view.update(cx, |view, _| {
            view.config.chat.layout = layout;
            view.theme = Theme::builtin(theme).unwrap();
        });
        let program = Program::new();
        let text = "Check $x^2$.\n\n```python\nprint('hello')\n```\n\n$$y=x+1$$";
        let _stream = overlay::attach_with(
            &view,
            &program,
            snapshot(cursor(1, 0), vec![user(1, text)]),
            cx,
        );
        wait_until(|| {
            cx.run_until_parked();
            overlay::poll(&view, &program, cx);
            cx.debug_bounds("chat-math-inline").is_some()
                && cx.debug_bounds("chat-math-display").is_some()
        });
        for selector in [
            mark,
            "chat-code-block",
            "chat-math-inline",
            "chat-math-display",
            "chat-copy-1",
        ] {
            assert!(cx.debug_bounds(selector).is_some(), "{selector}");
        }
        if layout == ChatLayoutMode::Document {
            assert!(cx.debug_bounds("chat-user-label").is_some());
        }
        let copy = cx.debug_bounds("chat-copy-1").unwrap();
        cx.simulate_mouse_move(copy.center(), None, Modifiers::default());
        overlay::poll(&view, &program, cx);
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
            text
        );
    }
}

#[test]
fn a_users_pasted_card_or_page_stays_inert_code() {
    let card = r#"```herdr-ui
{"title":"Pick","components":[{"type":"action","label":"Go","prompt":"go"}]}
```"#;
    let page = "```html\n<p>hello</p>\n```";
    let text = format!("{card}\n\n{page}");
    let mut transcript = Transcript::new("Codex");
    transcript.entries = vec![
        user(1, &text),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent { text: text.clone() },
        },
    ];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1), (EntryId(2), 1)]);
    let parts = |id| {
        let body = prepared.body(EntryId(id)).unwrap();
        let Body::Message(parts) = &*body else {
            panic!("message")
        };
        parts
            .iter()
            .map(|part| matches!(part, Part::Widget(_) | Part::Html { .. }))
            .collect::<Vec<_>>()
    };
    assert!(
        parts(1).iter().all(|live| !live),
        "the user's text stays code"
    );
    assert_eq!(parts(2).iter().filter(|live| **live).count(), 2);
}

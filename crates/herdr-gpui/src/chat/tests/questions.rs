use super::*;
use crate::{
    chat::prepared::{Body, Part, Prepared, answers_text},
    config::ChatLayoutMode,
};
use gpui::{Modifiers, TestAppContext, px, size};
use herdr_chat_protocol::{MAX_QUESTION_ANSWERS, MAX_QUESTION_TEXT, NoticeLevel, QuestionAnswer};
use herdr_client::{ConnectTarget, protocol::SurfaceRect};
use std::rc::Rc;

const RAW: &str = "<send_user_message_question_reply>\n[{\"question\":\"Synthetic question\",\"answer\":\"Selected answer\"}]\n</send_user_message_question_reply>";

fn answers() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            question: "What is $x^2$ when $x=2$?".into(),
            answer: "Use **four**.".into(),
        },
        QuestionAnswer {
            question: "How should the example print the result?".into(),
            answer: "```python\nprint(2 ** 2)\n```".into(),
        },
    ]
}

fn answered(answers: Vec<QuestionAnswer>) -> Entry {
    Entry {
        id: EntryId(1),
        kind: EntryKind::Notice {
            answers,
            details: Some(RAW.into()),
            level: NoticeLevel::Info,
            text: "Questions answered".into(),
        },
    }
}

#[test]
fn answers_prepare_rich_content_and_readable_copy_once_without_widgets_or_pages() {
    let mut answers = answers();
    answers[1].answer.push_str("\n\n```herdr-ui\n{\"type\":\"button\",\"label\":\"Never run\"}\n```\n\n```html\n<button>Never run</button>\n```");
    let expected = answers_text(&answers);
    let mut transcript = Transcript::new("Codex");
    transcript.entries.push(answered(answers));
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    let first = prepared.body(EntryId(1)).unwrap();
    let Body::Answers { text, parts } = &*first else {
        panic!("prepared answers")
    };
    assert_eq!(text.as_ref(), expected);
    assert!(!text.contains("send_user_message_question_reply"));
    let blocks: Vec<_> = parts
        .iter()
        .flat_map(|part| match part {
            Part::Rich(blocks) => blocks.iter(),
            _ => panic!("answers use only rich Markdown, never widgets, pages or remote images"),
        })
        .collect();
    assert!(blocks.iter().any(
        |block| matches!(block, rich::Block::Paragraph { content, .. }
        if content.iter().any(|part| matches!(part, rich::Inline::Math { .. })))
    ));
    for language in ["python", "herdr-ui", "html"] {
        assert!(
            blocks.iter().any(
                |block| matches!(block, rich::Block::Code { language: found, .. }
            if found == language)
            ),
            "{language}"
        );
    }
    assert_eq!(text.matches("**Question ").count(), 2);
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    assert!(Rc::ptr_eq(&first, &prepared.body(EntryId(1)).unwrap()));
    if let EntryKind::Notice { answers, .. } = &mut transcript.entries[0].kind {
        answers[0].answer = "Use **nine**.".into();
    }
    prepared.update(&transcript, &[(EntryId(1), 2)]);
    let next = prepared.body(EntryId(1)).unwrap();
    assert!(!Rc::ptr_eq(&first, &next));
    assert!(matches!(next.as_ref(), Body::Answers { text, .. } if text.contains("Use **nine**.")));
    if let EntryKind::Notice { answers, .. } = &mut transcript.entries[0].kind {
        answers.clear();
    }
    prepared.update(&transcript, &[(EntryId(1), 3)]);
    assert!(prepared.body(EntryId(1)).is_none());
}

#[test]
fn answers_copy_and_preparation_share_count_and_utf8_text_limits() {
    let too_many = vec![
        QuestionAnswer {
            question: "Question".into(),
            answer: "Answer".into()
        };
        MAX_QUESTION_ANSWERS + 1
    ];
    let text = answers_text(&too_many);
    assert_eq!(text.matches("**Question ").count(), MAX_QUESTION_ANSWERS);
    assert!(text.ends_with("… (answers truncated)"));
    let huge = vec![QuestionAnswer {
        question: "α".repeat(MAX_QUESTION_TEXT),
        answer: "This suffix exceeds the aggregate budget".into(),
    }];
    let text = answers_text(&huge);
    assert!(text.len() <= MAX_QUESTION_TEXT + MAX_QUESTION_ANSWERS * 64 + 64);
    assert!(text.ends_with("… (answers truncated)"));
    assert!(!text.contains("This suffix"));
}

#[gpui::test]
fn answered_questions_show_rich_text_and_copy_answers_or_original_source(cx: &mut TestAppContext) {
    for (layout, theme) in [
        (ChatLayoutMode::Timeline, "Default"),
        (ChatLayoutMode::IMessage, "Default"),
        (ChatLayoutMode::IMessage, "Catppuccin Latte"),
    ] {
        let answers = answers();
        let expected = answers_text(&answers);
        let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
        view.update(cx, |view, _| {
            view.config.chat.layout = layout;
            view.theme = crate::config::Theme::builtin(theme).unwrap();
        });
        let program = Program::new();
        let _stream = overlay::attach_with(
            &view,
            &program,
            snapshot(cursor(1, 0), vec![answered(answers)]),
            cx,
        );
        wait_until(|| {
            cx.run_until_parked();
            overlay::poll(&view, &program, cx);
            cx.debug_bounds("chat-math-inline").is_some()
        });
        assert!(cx.debug_bounds("chat-answers-1").is_some());
        assert!(cx.debug_bounds("chat-code-block").is_some());
        assert!(cx.debug_bounds("chat-notice-details-1").is_none());
        let copy = cx.debug_bounds("chat-copy-answers").unwrap();
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
            expected
        );
        let record = cx.debug_bounds("chat-notice-1").unwrap();
        cx.simulate_click(record.center(), Modifiers::default());
        overlay::poll(&view, &program, cx);
        let source = cx.debug_bounds("chat-notice-source-1").unwrap();
        cx.simulate_mouse_move(source.center(), None, Modifiers::default());
        overlay::poll(&view, &program, cx);
        let copy = cx.debug_bounds("chat-copy-1").unwrap();
        cx.simulate_click(copy.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
            RAW
        );
        let record = cx.debug_bounds("chat-notice-1").unwrap();
        cx.simulate_click(record.center(), Modifiers::default());
        overlay::poll(&view, &program, cx);
        assert!(cx.debug_bounds("chat-notice-details-1").is_none());
        assert!(cx.debug_bounds("chat-copy-answers").is_some());
    }
}

#[gpui::test]
fn answered_questions_use_the_user_layout_and_fit_a_narrow_pane(cx: &mut TestAppContext) {
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    view.update(cx, |view, cx| {
        view.sidebar_width = Some(150.);
        view.config.sidebar_style.activity_bar = false;
        cx.notify();
    });
    cx.simulate_resize(size(px(650.), px(860.)));
    cx.run_until_parked();
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![answered(vec![
                QuestionAnswer {
                    question: "Which output should this example produce?".into(),
                    answer: "Show the calculation with clear explanations.".into(),
                },
                QuestionAnswer {
                    question: "Which format should it use?".into(),
                    answer: "Use Markdown.".into(),
                },
            ])],
        ),
        cx,
    );
    view.update(cx, |view, _| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.frame.width = view.options.surface_size.cols;
        surface.frame.height = view.options.surface_size.rows;
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width: surface.frame.width,
            height: surface.frame.height,
        };
        surface.panes[0].rect = rect;
        surface.panes[0].inner_rect = rect;
    });
    for (mode, mark) in [
        (ChatLayoutMode::Timeline, "chat-avatar-1"),
        (ChatLayoutMode::Bubbles, "chat-bubble-1"),
        (ChatLayoutMode::Terminal, "chat-mark-1"),
        (ChatLayoutMode::Document, "chat-heading-1"),
        (ChatLayoutMode::Cards, "chat-card-1"),
        (ChatLayoutMode::IMessage, "chat-imessage-1"),
    ] {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(mode, |_| Ok(()), cx)
        });
        overlay::poll(&view, &program, cx);
        let card = cx.debug_bounds("chat-answers-1").unwrap();
        let pane = cx.debug_bounds("chat-view").unwrap();
        assert!(
            card.left() >= pane.left() && card.right() <= pane.right(),
            "{mode:?}: {card:?} in {pane:?}"
        );
        assert!(
            card.size.width >= px(280.),
            "{mode:?}: collapsed card {card:?}"
        );
        assert!(
            card.size.height < px(550.),
            "{mode:?}: excessive wrapping {card:?}"
        );
        assert!(cx.debug_bounds(mark).is_some(), "{mode:?}");
        assert!(cx.debug_bounds("chat-notice-details-1").is_none());
        if mode == ChatLayoutMode::IMessage {
            let bubble = cx.debug_bounds(mark).unwrap();
            assert!(
                bubble.center().x > pane.center().x,
                "answers on the user's side"
            );
            let record = cx.debug_bounds("chat-notice-1").unwrap();
            cx.simulate_click(record.center(), Modifiers::default());
            overlay::poll(&view, &program, cx);
            let source = cx.debug_bounds("chat-notice-source-1").unwrap();
            cx.simulate_mouse_move(source.center(), None, Modifiers::default());
            overlay::poll(&view, &program, cx);
            let details = cx.debug_bounds("chat-notice-details-1").unwrap();
            let copy = cx.debug_bounds("chat-copy-1").unwrap();
            assert!(
                copy.left() >= details.left()
                    && copy.right() <= details.right()
                    && copy.top() >= details.top()
                    && copy.bottom() <= details.bottom(),
                "source copy stays inside its disclosure: {copy:?} in {details:?}"
            );
            cx.simulate_click(copy.center(), Modifiers::default());
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                RAW
            );
        }
    }
}

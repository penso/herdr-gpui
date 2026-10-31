use super::*;
use crate::chat::visual::Output;

fn code(n: usize) -> Source {
    Source::Code {
        language: "rust".into(),
        source: format!("let value_{n} = {n};").into(),
    }
}

#[gpui::test]
fn exact_screenshot_math_and_later_code_render_and_scrollback_recovers(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut transcript = Transcript::new("Codex");
    transcript.entries = (0..visual::MAX_RICH_VISUALS)
        .map(|n| agent(n as u64 + 1, &format!("$x_{{{n}}}$")))
        .collect();
    let latest = EntryId(visual::MAX_RICH_VISUALS as u64 + 1);
    transcript.entries.push(agent(latest.0, "| Coefficients | Value |\n|---|---|\n| Head | \\(A, C, G, b\\) |\n\nThis **F3/E1, \\(d4/\\chi4/R2\\)** seed.\n\n```rust\nlet recovered = 42;\n```"));
    let keys: Vec<_> = transcript
        .entries
        .iter()
        .map(|entry| (entry.id, 1))
        .collect();
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    let before = prepared.body(latest).unwrap();
    let formulas = [
        VisualId::of(("math", rich::MathMode::Inline, "A, C, G, b")),
        VisualId::of(("math", rich::MathMode::Inline, r"d4/\chi4/R2")),
    ];
    let code = VisualId::of(("code", "rust", "let recovered = 42;\n"));
    assert!(formulas.iter().all(|id| !prepared.visuals.wants(*id)));
    prepared.visuals.begin_frame();
    prepared.request_visible(latest, false);
    for _ in 0..3 {
        let jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
        assert!(jobs.len() <= 2);
        for job in jobs {
            let (id, ticket, output) = job.run();
            prepared.visuals.finish(id, ticket, output);
        }
    }
    assert!(
        formulas
            .iter()
            .all(|id| matches!(prepared.visuals.shown(*id), Shown::Ready(_)))
    );
    assert!(prepared.visuals.code(code).is_some());
    assert!(std::rc::Rc::ptr_eq(
        &before,
        &prepared.body(latest).unwrap()
    ));
    assert_eq!(prepared.visuals.rich_budget().0, visual::MAX_RICH_VISUALS);
    let missing = (0..visual::MAX_RICH_VISUALS)
        .find(|n| {
            !prepared.visuals.wants(VisualId::of((
                "math",
                rich::MathMode::Inline,
                format!("x_{{{n}}}"),
            )))
        })
        .unwrap();
    let old = VisualId::of(("math", rich::MathMode::Inline, format!("x_{{{missing}}}")));
    prepared.visuals.begin_frame();
    prepared.request_visible(EntryId(missing as u64 + 1), false);
    let jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert!(jobs.iter().any(|job| job.id == old));
    for job in jobs {
        let (id, ticket, output) = job.run();
        prepared.visuals.finish(id, ticket, output);
    }
    assert!(matches!(prepared.visuals.shown(old), Shown::Ready(_)));
}

#[gpui::test]
fn viewport_overflow_is_stable_and_evicted_running_tickets_stay_occupied(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut visuals = Visuals::default();
    let ids: Vec<_> = (0..visual::MAX_RICH_VISUALS).map(VisualId::of).collect();
    for (n, id) in ids.iter().enumerate() {
        visuals.want(*id, code(n));
    }
    let running = visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(running.len(), 2);
    visuals.begin_frame();
    for (n, id) in ids
        .iter()
        .enumerate()
        .filter(|(_, id)| !running.iter().any(|job| job.id == **id))
    {
        visuals.request_visible(*id, code(n));
    }
    let fresh = [VisualId::of("fresh one"), VisualId::of("fresh two")];
    for id in fresh {
        visuals.request_visible(id, code(500));
    }
    let overflow = VisualId::of("overflow");
    visuals.request_visible(overflow, code(501));
    assert!(visuals.jobs(&styled(0xffffff), &renderer).is_empty());
    assert!(fresh.iter().all(|id| visuals.wants(*id)));
    assert!(!visuals.wants(overflow));
    assert!(running.iter().all(|job| !visuals.wants(job.id)));
    // Reintroduce an evicted identity before its old ticket returns.
    let old = running[0].id;
    visuals.begin_frame();
    visuals.request_visible(old, code(700));
    assert!(visuals.jobs(&styled(0xffffff), &renderer).is_empty());
    for job in running {
        assert!(!visuals.finish(job.id, job.ticket, Ok(Output::Code(Vec::new()))));
    }
    assert!(visuals.code(old).is_none());
    let jobs = visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().any(|job| job.id == old));
    for job in jobs {
        visuals.finish(job.id, job.ticket, Ok(Output::Code(Vec::new())));
    }
    assert!(visuals.code(old).is_some());
    assert_eq!(visuals.rich_budget().0, visual::MAX_RICH_VISUALS);
}

#[gpui::test]
fn visible_math_reclaims_offscreen_texture_bytes_and_budget_failures_retry_once_room_exists(
    cx: &mut TestAppContext,
) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut visuals = Visuals::default();
    let texture = Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
        image::RgbaImage::new(1024, 1024),
    )]));
    let bytes = texture.as_bytes(0).unwrap().len();
    let capacity = visual::MAX_MATH_TEXTURE_BYTES / bytes;
    let drawn = visual::Drawn {
        texture,
        width: 512.,
        height: 512.,
        baseline: Some(20.),
    };
    let source = Source::Math {
        source: "x".into(),
        mode: rich::MathMode::Inline,
    };
    let ids: Vec<_> = (0..=capacity)
        .map(|n| VisualId::of(("budget", n)))
        .collect();
    for id in &ids {
        visuals.want(*id, source.clone());
    }
    loop {
        let jobs = visuals.jobs(&styled(0xffffff), &renderer);
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn.clone())));
        }
    }
    let refused = *ids
        .iter()
        .find(|id| matches!(visuals.shown(**id), Shown::Failed(Error::MathLimit)))
        .unwrap();
    visuals.begin_frame();
    for id in &ids {
        visuals.request_visible(*id, source.clone());
    }
    assert!(
        visuals.jobs(&styled(0xffffff), &renderer).is_empty(),
        "no repeated retry while every texture remains visible"
    );
    visuals.begin_frame();
    visuals.request_visible(refused, source.clone());
    let jobs = visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, refused);
    for job in jobs {
        assert!(visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn.clone()))));
    }
    assert!(matches!(visuals.shown(refused), Shown::Ready(_)));
    let fresh = VisualId::of("fresh visible math");
    visuals.begin_frame();
    visuals.request_visible(fresh, source);
    let jobs = visuals.jobs(&styled(0xffffff), &renderer);
    for job in jobs {
        assert!(visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn.clone()))));
    }
    assert!(matches!(visuals.shown(fresh), Shown::Ready(_)));
    assert!(visuals.rich_budget().1 <= visual::MAX_MATH_TEXTURE_BYTES);
    assert!(!visuals.take_released().is_empty());
}

#[gpui::test]
fn visible_rehydration_preserves_open_fence_and_per_message_code_limits(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let text = (0..33)
        .map(|n| format!("```rust\nlet value_{n} = {n};\n```\n\n"))
        .collect::<String>()
        + "```rust\nunfinished";
    let (mut prepared, _) = message(&text);
    prepared.visuals.clear();
    prepared.request_visible(EntryId(1), false);
    let jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(jobs.len(), 2);
    assert_eq!(prepared.visuals.rich_budget().0, 32);
    assert!(
        !prepared
            .visuals
            .wants(VisualId::of(("code", "rust", "let value_32 = 32;\n")))
    );
    assert!(
        !prepared
            .visuals
            .wants(VisualId::of(("code", "rust", "unfinished")))
    );
}

#[gpui::test]
fn collapsed_tools_request_no_code_jobs_and_table_math_keeps_display_style(
    cx: &mut TestAppContext,
) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut transcript = Transcript::new("Test");
    transcript.entries.push(Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(herdr_chat_protocol::Tool {
            parent: None,
            call_id: "many".into(),
            title: "Edit".into(),
            kind: herdr_chat_protocol::ToolKind::Edit,
            status: herdr_chat_protocol::ToolStatus::Completed,
            locations: Vec::new(),
            content: (0..16)
                .map(|n| herdr_chat_protocol::ToolContent::Text {
                    text: format!("```rust\nlet value_{n} = {n};\n```"),
                })
                .collect(),
        }),
    });
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    prepared.visuals.clear();
    prepared.request_visible(EntryId(1), false);
    let jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert!(jobs.is_empty());
    assert_eq!(prepared.visuals.rich_budget().0, 0);
    prepared.visuals.begin_frame();
    prepared.request_visible(EntryId(1), true);
    prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(prepared.visuals.rich_budget().0, 16);

    let (mut prepared, _) = message("| Formula |\n|---|\n| $$\\frac{a}{b}$$ |\n");
    let body = prepared.body(EntryId(1)).unwrap();
    let Body::Message(parts) = &*body else {
        panic!("message");
    };
    assert!(parts.iter().any(|part| matches!(part, Part::Rich(blocks) if blocks.iter().any(|block| matches!(block, rich::Block::Table { rows, .. } if rows.iter().flatten().flatten().any(|inline| matches!(inline, rich::Inline::Math { mode: rich::MathMode::Display, .. })))))));
    prepared.visuals.clear();
    prepared.request_visible(EntryId(1), false);
    let jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(jobs.len(), 1);
    let (id, ticket, result) = jobs.into_iter().next().unwrap().run();
    let height = match &result {
        Ok(Output::Image(drawn)) => drawn.height,
        _ => panic!("display formula renders"),
    };
    let size = Config::default().ui.size;
    let ink = Theme::default().foreground;
    let display = rich::draw_math(r"\frac{a}{b}", rich::MathMode::Display, size, ink).unwrap();
    let inline = rich::draw_math(r"\frac{a}{b}", rich::MathMode::Inline, size, ink).unwrap();
    assert_eq!(height, display.height);
    assert!(height > inline.height);
    assert!(prepared.visuals.finish(id, ticket, result));
}

#[gpui::test]
fn idle_lazy_list_render_and_scroll_schedule_visible_jobs_without_new_agent_output(
    cx: &mut TestAppContext,
) {
    use super::super::overlay::{attach_with, open, poll};
    let (view, _peer, cx) = open(cx, herdr_client::ConnectTarget::Local);
    let program = Program::new();
    let mut entries: Vec<_> = (0..visual::MAX_RICH_VISUALS)
        .map(|n| agent(n as u64 + 1, &format!("Earlier $x_{{{n}}}$ response.")))
        .collect();
    entries.push(agent(
        129,
        "This **F3/E1, \\(d4/\\chi4/R2\\)** seed.\n\n```rust\nlet visible = 1;\n```",
    ));
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    let latest = VisualId::of(("math", rich::MathMode::Inline, r"d4/\chi4/R2"));
    wait_until(|| {
        cx.run_until_parked();
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            matches!(
                view.chats
                    .pane("w1:p1")
                    .unwrap()
                    .prepared
                    .visuals
                    .shown(latest),
                Shown::Ready(_)
            )
        })
    });
    let missing = view.read_with(cx, |view, _| {
        let visuals = &view.chats.pane("w1:p1").unwrap().prepared.visuals;
        (0..visual::MAX_RICH_VISUALS)
            .find(|n| {
                !visuals.wants(VisualId::of((
                    "math",
                    rich::MathMode::Inline,
                    format!("x_{{{n}}}"),
                )))
            })
            .unwrap()
    });
    let old = VisualId::of(("math", rich::MathMode::Inline, format!("x_{{{missing}}}")));
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        pane.list.scroll_to(gpui::ListOffset {
            item_ix: missing,
            offset_in_item: gpui::px(0.),
        });
        pane.glide.user_scrolled();
        view.redraw_terminal(cx);
    });
    wait_until(|| {
        cx.run_until_parked();
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            matches!(
                view.chats
                    .pane("w1:p1")
                    .unwrap()
                    .prepared
                    .visuals
                    .shown(old),
                Shown::Ready(_)
            )
        })
    });
    assert!(cx.debug_bounds("chat-math-inline").is_some());
}

use super::*;
use crate::{
    chat::{
        prepared::{Body, Part, Prepared},
        visual::{self, DiagramStyle, Error, Shown, Source, VisualId, Visuals},
    },
    config::{Config, Theme},
};
use base64::Engine as _;
use gpui::TestAppContext;
use herdr_chat_protocol::{Author, Image};
use std::{collections::HashSet, io::Cursor};

const FLOW: &str = "flowchart LR\n  A[Ask] --> B[Draw]\n";

fn png(width: u32, height: u32) -> Image {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Image::new("image/png", data).unwrap()
}

pub(super) fn message(text: &str) -> (Prepared, Vec<String>) {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![agent(1, text)];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    let Body::Message(parts) = &*prepared.body(EntryId(1)).unwrap() else {
        panic!("not a message");
    };
    let shape = parts
        .iter()
        .map(|part| match part {
            Part::Text(lines) => format!("text:{}", lines[0].text),
            Part::Rich(blocks) => match blocks.first() {
                Some(rich::Block::Paragraph { content, .. }) => format!(
                    "text:{}",
                    content
                        .iter()
                        .filter_map(|part| match part {
                            rich::Inline::Text(text, _) => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<String>()
                ),
                Some(rich::Block::Code { source, .. }) => {
                    format!("text:{}", source.lines().next().unwrap_or(""))
                }
                _ => "rich".to_owned(),
            },
            Part::Widget(_) => "widget".to_owned(),
            Part::Diagram(..) => "diagram".to_owned(),
            Part::Html { .. } => "page".to_owned(),
            Part::Picture { alt } => format!("picture:{alt}"),
            Part::Remote { url, .. } => format!("remote:{}", url.as_str()),
        })
        .collect();
    (prepared, shape)
}

fn styled(background: u32) -> DiagramStyle {
    let theme = Theme {
        background,
        ..Theme::default()
    };
    DiagramStyle::of(&theme, &Config::default().ui)
}

#[test]
fn a_closed_mermaid_fence_becomes_a_diagram_between_text() {
    let (prepared, shape) = message(&format!("Here:\n```mermaid\n{FLOW}```\nDone."));
    assert_eq!(shape, ["text:Here:", "diagram", "text:Done."]);
    let id = VisualId::of(FLOW);
    assert!(matches!(prepared.visuals.shown(id), Shown::Pending));
    assert_eq!(prepared.drawing(id).collect::<Vec<_>>(), [EntryId(1)]);
}

#[test]
fn a_fence_still_streaming_or_too_long_stays_code() {
    let (_, shape) = message(&format!("```mermaid\n{FLOW}"));
    assert_eq!(shape, ["text:flowchart LR"]);
    let long = format!(
        "```mermaid\n{}\n```\n",
        "A-->B\n".repeat(visual::MAX_DIAGRAM_SOURCE)
    );
    let shape = message(&long).1;
    assert_eq!(shape.first().map(String::as_str), Some("text:A-->B"));
    assert!(!shape.iter().any(|part| part == "diagram"));
    // Other fences are code as before.
    assert_eq!(
        message("```rust\nfn main() {}\n```\n").1,
        ["text:fn main() {}"]
    );
}

#[test]
fn image_entries_and_tool_images_ask_for_visuals() {
    let image = png(2, 2);
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![Entry {
        id: EntryId(1),
        kind: EntryKind::Image {
            author: Author::Agent,
            image: image.clone(),
        },
    }];
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    let id = VisualId::of(&image.data);
    assert!(matches!(&*prepared.body(EntryId(1)).unwrap(), Body::Image(found) if *found == id));
    transcript.entries.clear();
    prepared.update(&transcript, &[]);
    assert!(prepared.drawing(id).next().is_none());
}

#[test]
fn an_image_decodes_at_its_pixel_size() {
    let drawn = visual::decode(&png(3, 2)).unwrap();
    assert_eq!((drawn.width, drawn.height), (3., 2.));
    let pixels = drawn.texture.as_bytes(0).unwrap();
    // GPUI takes BGRA.
    assert_eq!(&pixels[..4], &[30, 30, 200, 255]);
    let bad = Image::new("image/png", "AAAA".to_owned()).unwrap();
    assert!(matches!(visual::decode(&bad), Err(Error::Decode(_))));
    let odd = Image::new("image/png", "A".to_owned()).unwrap();
    assert!(matches!(visual::decode(&odd), Err(Error::Base64(_))));
}

#[gpui::test]
fn diagrams_are_drawn_off_thread_and_redrawn_for_a_new_theme(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut visuals = Visuals::default();
    let id = VisualId::of(FLOW);
    visuals.want(id, Source::Diagram(FLOW.into()));
    let sequence = "sequenceDiagram\n  participant U as You\n  U->>A: prompt\n  A-->>U: answer\n";
    let other = VisualId::of(sequence);
    visuals.want(other, Source::Diagram(sequence.into()));
    let broken = VisualId::of("nope");
    visuals.want(broken, Source::Diagram("this is not a diagram\n".into()));
    let style = styled(0xff_ff_ff);
    // Two run at once; the third starts once one lands.
    for _ in 0..2 {
        let jobs = visuals.jobs(&style, &renderer);
        assert!(!jobs.is_empty() && jobs.len() <= 2);
        assert!(
            visuals.jobs(&style, &renderer).is_empty(),
            "the slots are taken"
        );
        for job in jobs {
            let (job_id, ticket, drawn) = job.run();
            assert!(visuals.finish(job_id, ticket, drawn));
        }
    }
    assert!(matches!(visuals.shown(other), Shown::Ready(_)));
    let Shown::Ready(drawn) = visuals.shown(id) else {
        panic!("the flowchart was not drawn");
    };
    assert!(drawn.width > 0. && drawn.height > 0.);
    let pixels = drawn.texture.as_bytes(0).unwrap();
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    assert!(matches!(
        visuals.shown(broken),
        Shown::Failed(Error::Diagram(_))
    ));

    // A theme change redraws the diagram; the old texture stays until the
    // new one lands, and an overtaken job's texture is released.
    let dark = styled(0x10_10_10);
    let jobs = visuals.jobs(&dark, &renderer);
    assert_eq!(jobs.len(), 2);
    assert!(matches!(visuals.shown(id), Shown::Ready(_)));
    let (job_id, ticket, drawn) = jobs
        .into_iter()
        .map(|job| job.run())
        .find(|(_, _, drawn)| drawn.is_ok())
        .unwrap();
    assert!(!visuals.finish(job_id, ticket + 100, drawn));
    assert_eq!(visuals.take_released().len(), 1);

    visuals.retain(&HashSet::new());
    assert!(matches!(visuals.shown(id), Shown::Pending));
    assert!(!visuals.take_released().is_empty());
}

#[gpui::test]
fn forgotten_streamed_work_keeps_its_background_slot(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut visuals = Visuals::default();
    for source in ["x", "y"] {
        visuals.want(
            VisualId::of(source),
            Source::Math {
                source: source.into(),
                mode: rich::MathMode::Inline,
            },
        );
    }
    let jobs = visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(jobs.len(), 2);
    visuals.clear();
    visuals.want(
        VisualId::of("z"),
        Source::Math {
            source: "z".into(),
            mode: rich::MathMode::Inline,
        },
    );
    assert!(visuals.jobs(&styled(0xffffff), &renderer).is_empty());
    for job in jobs {
        let (id, ticket, result) = job.run();
        assert!(!visuals.finish(id, ticket, result));
    }
    assert_eq!(visuals.jobs(&styled(0xffffff), &renderer).len(), 1);
}

#[gpui::test]
fn rich_table_columns_align_and_fenced_code_copies_exactly(cx: &mut TestAppContext) {
    use super::overlay::{attach_with, open, poll};
    use gpui::Modifiers;
    let (view, _peer, cx) = open(cx, herdr_client::ConnectTarget::Local);
    let program = Program::new();
    let code = "let α = 2;\nprintln!(\"{α}\");\n";
    let text =
        format!("| Name | Value |\n| :--- | ---: |\n| longer name | 42 |\n\n```rust\n{code}```");
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &text)]),
        cx,
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-copy-code").is_some()
    });
    let header = cx.debug_bounds("chat-table-cell-0-1").unwrap();
    let value = cx.debug_bounds("chat-table-cell-1-1").unwrap();
    assert_eq!(header.left(), value.left());
    assert_eq!(header.size.width, value.size.width);
    let table = cx.debug_bounds("chat-rich-table").unwrap();
    assert!(
        (header.right() - table.right()).abs() <= gpui::px(2.),
        "the final column fills the table, allowing its border"
    );
    let copy = cx.debug_bounds("chat-copy-code").unwrap();
    cx.simulate_click(copy.center(), Modifiers::default());
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .as_deref(),
        Some(code)
    );
}

#[gpui::test]
fn multiline_display_math_and_inline_math_have_distinct_native_layouts(cx: &mut TestAppContext) {
    use super::overlay::{attach_with, open, poll};
    let (view, _peer, cx) = open(cx, herdr_client::ConnectTarget::Local);
    let program = Program::new();
    let text = "Before $x^2$ after.\n\n$$\n\\begin{aligned}a &= b \\\\ c &= d\\end{aligned}\n$$";
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, text)]),
        cx,
    );
    wait_until(|| {
        // GPUI's test executor only advances when explicitly pumped. Polling
        // sockets and drawing alone leaves the raster jobs pending forever.
        cx.run_until_parked();
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let prepared = &view.chats.pane("w1:p1").unwrap().prepared;
            let body = prepared.body(EntryId(1)).unwrap();
            let Body::Message(parts) = &*body else {
                panic!("expected the math message");
            };
            for part in parts {
                if let Part::Rich(blocks) = part {
                    for id in blocks.iter().flat_map(rich::Block::visuals) {
                        if let Shown::Failed(error) = prepared.visuals.shown(id) {
                            panic!("math raster failed: {error}");
                        }
                    }
                }
            }
        });
        cx.debug_bounds("chat-math-inline").is_some()
            && cx.debug_bounds("chat-math-display").is_some()
    });
    let inline = cx.debug_bounds("chat-math-inline").unwrap();
    let display = cx.debug_bounds("chat-math-display").unwrap();
    assert!(inline.size.width > gpui::px(0.));
    assert!(display.top() >= inline.bottom());
    assert!(display.size.height > inline.size.height);
}

#[gpui::test]
fn replacing_a_full_rich_transcript_admits_new_math_without_losing_worker_slots(
    cx: &mut TestAppContext,
) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut transcript = Transcript::new("Claude");
    transcript.entries = (0..visual::MAX_RICH_VISUALS)
        .map(|n| agent(n as u64 + 1, &format!("$x_{{{n}}}$")))
        .collect();
    let keys: Vec<_> = transcript
        .entries
        .iter()
        .map(|entry| (entry.id, 1))
        .collect();
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    assert_eq!(prepared.visuals.rich_budget().0, visual::MAX_RICH_VISUALS);
    let old_jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(old_jobs.len(), 2);

    transcript.entries = vec![agent(1, "$y$")];
    prepared.update(&transcript, &[(EntryId(1), 2)]);
    let replacement = VisualId::of(("math", rich::MathMode::Inline, "y"));
    assert!(prepared.visuals.wants(replacement));
    assert_eq!(prepared.visuals.rich_budget().0, 1);
    assert!(
        prepared
            .visuals
            .jobs(&styled(0xffffff), &renderer)
            .is_empty()
    );
    for job in old_jobs {
        let (id, ticket, output) = job.run();
        assert!(!prepared.visuals.finish(id, ticket, output));
    }
    let jobs = prepared.visuals.jobs(&styled(0xffffff), &renderer);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, replacement);
}

#[gpui::test]
fn rich_cache_admission_and_retained_math_bytes_are_bounded(cx: &mut TestAppContext) {
    use visual::{Drawn, MAX_MATH_TEXTURE_BYTES, MAX_RICH_VISUALS, Output};
    let renderer = cx.update(|cx| cx.svg_renderer());
    let mut visuals = Visuals::default();
    for n in 0..=MAX_RICH_VISUALS {
        visuals.want(
            VisualId::of(n),
            Source::Code {
                language: "rs".into(),
                source: "let x = 1;".into(),
            },
        );
    }
    assert_eq!(visuals.rich_budget(), (MAX_RICH_VISUALS, 0));
    assert!(!visuals.wants(VisualId::of(MAX_RICH_VISUALS)));
    let picture = VisualId::of("ordinary image");
    visuals.want(picture, Source::Image(png(1, 1)));
    assert!(
        visuals.wants(picture),
        "the rich budget does not reject existing image types"
    );
    visuals.clear();
    assert_eq!(visuals.rich_budget(), (0, 0));

    // One shared 4 MiB fixture stands in for distinct worker textures. The
    // accountant must charge each entry, independently of Arc sharing.
    let texture = Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
        image::RgbaImage::new(1024, 1024),
    )]));
    let bytes = texture.as_bytes(0).unwrap().len();
    let capacity = MAX_MATH_TEXTURE_BYTES / bytes;
    let drawn = Drawn {
        texture,
        width: 512.,
        height: 512.,
        baseline: Some(20.),
    };
    let ids: Vec<_> = (0..=capacity)
        .map(|n| VisualId::of(("math-budget", n)))
        .collect();
    for id in &ids {
        visuals.want(
            *id,
            Source::Math {
                source: "x".into(),
                mode: rich::MathMode::Inline,
            },
        );
    }
    loop {
        let jobs = visuals.jobs(&styled(0xffffff), &renderer);
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            assert!(visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn.clone()))));
        }
    }
    assert_eq!(
        visuals.rich_budget(),
        (capacity + 1, MAX_MATH_TEXTURE_BYTES)
    );
    let refused = ids
        .iter()
        .find(|id| matches!(visuals.shown(**id), Shown::Failed(Error::MathLimit)))
        .copied()
        .unwrap();
    assert!(
        visuals.jobs(&styled(0xffffff), &renderer).is_empty(),
        "limit failures do not retry every tick"
    );
    let jobs = visuals.jobs(&styled(0x101010), &renderer);
    assert!(
        jobs.iter().all(|job| job.id != refused),
        "theme changes do not restart budget failures"
    );
    visuals.retain(&HashSet::from([refused]));
    assert_eq!(visuals.rich_budget(), (1, 0));
    for job in jobs {
        assert!(!visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn.clone()))));
    }
    visuals.clear();
    assert_eq!(visuals.rich_budget(), (0, 0));
    let fresh = VisualId::of("new formula");
    visuals.want(
        fresh,
        Source::Math {
            source: "y".into(),
            mode: rich::MathMode::Inline,
        },
    );
    assert!(
        visuals
            .jobs(&styled(0x101010), &renderer)
            .iter()
            .any(|job| job.id == fresh)
    );
}

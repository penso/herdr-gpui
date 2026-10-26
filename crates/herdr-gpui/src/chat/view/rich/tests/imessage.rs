use super::*;
use crate::chat::visual::{DiagramStyle, Drawn, Output, Source};
use core::prelude::v1::test;
use std::sync::Arc;

#[gpui::test]
fn imessage_prose_inherits_its_bubble_while_rich_surfaces_keep_theme_ink(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in ["Default", "Catppuccin Latte"] {
        view.update(cx, |view, cx| {
            let Some(theme) = Theme::builtin(name) else {
                panic!("missing {name}")
            };
            view.theme = theme;
            view.config.chat.layout = ChatLayoutMode::IMessage;
            let ink = Some(rgb(view.theme.foreground).into());
            let mut visuals = Visuals::default();
            let document = crate::chat::selection::Document::default();
            let selected = crate::chat::selection::Selection::default();
            let geometry = super::super::super::selection::Geometry::default();
            let selection = Scope::new(
                "fixture",
                herdr_chat_protocol::EntryId(0),
                crate::chat::selection::Region::Body(0),
                &document,
                &selected,
                &geometry,
            );
            let mut prose = view.render_chat_rich("fixture", &[], &visuals, &selection, cx);
            assert_eq!(prose.text_style().color, None, "{name}: inherit bubble ink");
            for style in [
                Style::default(),
                Style {
                    link: Some("https://example.org".into()),
                    ..Style::default()
                },
            ] {
                assert_eq!(
                    view.rich_style(&style).color,
                    None,
                    "{name}: prose/link ink"
                );
            }
            let inline_code = view.rich_style(&Style {
                code: true,
                ..Style::default()
            });
            assert_eq!(inline_code.color, ink);
            assert!(crate::contrast::ratio(view.theme.foreground, view.theme.active) >= 4.5);

            let source: Arc<str> = "plain code".into();
            let (mut code, mut table) = {
                let links = Links::new(view, "fixture", cx);
                let mut context = RichContext {
                    visuals: &visuals,
                    links,
                    selection: &selection,
                    cx,
                };
                (
                    view.rich_code(VisualId::of("code"), "", &source, 0, &mut context),
                    view.rich_table(&[], &[], 0, &mut context),
                )
            };
            assert_eq!(code.text_style().color, ink, "{name}: unhighlighted code");
            assert_eq!(
                code.style().background,
                Some(rgb(view.theme.surface).into())
            );
            assert!(crate::contrast::ratio(view.theme.foreground, view.theme.surface) >= 4.5);
            assert_eq!(table.text_style().color, ink);
            assert_eq!(
                table.style().background,
                Some(rgb(view.theme.background).into())
            );

            let math = VisualId::of("x");
            visuals.want(
                math,
                Source::Math {
                    source: "x".into(),
                    mode: crate::chat::rich::MathMode::Inline,
                },
            );
            // Both source fallbacks and finished transparent textures need
            // a backdrop matching the ink their worker chose.
            for ready in [false, true] {
                if ready {
                    let jobs =
                        visuals.jobs(&DiagramStyle::of(&view.theme, &view.config.ui), &renderer);
                    assert!(!jobs.is_empty());
                    for job in jobs {
                        let drawn = Drawn {
                            texture: Arc::new(RenderImage::new(vec![image::Frame::new(
                                image::RgbaImage::new(1, 1),
                            )])),
                            width: 1.,
                            height: 1.,
                            baseline: Some(1.),
                        };
                        assert!(visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn))));
                    }
                }
                for inline in [false, true] {
                    let mut panel = view.rich_math(math, "x", &visuals, inline);
                    assert_eq!(
                        panel.style().background,
                        Some(rgb(view.theme.background).into())
                    );
                }
            }
            assert!(crate::contrast::ratio(view.theme.foreground, view.theme.background) >= 4.5);
            let mut copy = view.rich_copy(copy::ANSWERS, source, cx);
            assert_eq!(
                copy.style().background,
                Some(rgb(view.theme.background).into())
            );

            view.config.chat.layout = ChatLayoutMode::Document;
            let mut prose = view.render_chat_rich("fixture", &[], &visuals, &selection, cx);
            assert_eq!(
                prose.text_style().color,
                ink,
                "other layouts retain theme ink"
            );
            let mut math = view.rich_math(math, "x", &visuals, true);
            assert!(
                math.style().background.is_none(),
                "other layouts need no panel"
            );
        });
    }
}

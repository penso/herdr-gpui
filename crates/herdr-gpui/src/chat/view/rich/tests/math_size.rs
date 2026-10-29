//! A formula keeps the size of the text it sits in, so a layout that sets
//! a message smaller does not leave its formulas at full size.

use super::*;
use crate::chat::visual::{DiagramStyle, Drawn, Output, Source};
use core::prelude::v1::test;
use std::sync::Arc;

#[gpui::test]
fn a_formula_scales_with_its_message(cx: &mut TestAppContext) {
    let renderer = cx.update(|cx| cx.svg_renderer());
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, _| {
        let mut visuals = Visuals::default();
        let math = VisualId::of("x");
        visuals.want(
            math,
            Source::Math {
                source: "x".into(),
                mode: crate::chat::rich::MathMode::Inline,
            },
        );
        for job in visuals.jobs(&DiagramStyle::of(&view.theme, &view.config.ui), &renderer) {
            let drawn = Drawn {
                texture: Arc::new(RenderImage::new(vec![image::Frame::new(
                    image::RgbaImage::new(1, 1),
                )])),
                width: 40.,
                height: 20.,
                baseline: Some(16.),
            };
            assert!(visuals.finish(job.id, job.ticket, Ok(Output::Image(drawn))));
        }
        let full = view.chat_text_size();
        let width = |size: f32| {
            view.rich_math(math, "x", &visuals, true, size)
                .style()
                .size
                .width
        };
        assert_eq!(width(full), Some(px(40.).into()));
        assert_eq!(
            width(full / 2.),
            Some(px(20.).into()),
            "half the text, half the formula"
        );
    });
}

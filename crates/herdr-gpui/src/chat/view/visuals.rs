//! Pictures and diagrams in the entry list, drawn from textures the
//! background prepared, and the agent message that holds diagrams.

use super::super::{
    prepared::Part,
    selection::Region,
    visual::{Shown, VisualId, Visuals},
};
use crate::{HerdrWindow, config::corners, release_notes};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Author, EntryId};

/// Widest a picture in a message is drawn, in points.
const IMAGE_WIDTH: f32 = 560.;
/// Widest a picture under a tool call is drawn, in points.
const TOOL_IMAGE_WIDTH: f32 = 240.;
/// Height of the box a visual takes while it is prepared.
const PENDING_HEIGHT: f32 = 48.;

/// How a visual sits in its row.
#[derive(Clone, Copy)]
pub(super) enum Frame {
    Diagram,
    Image,
    ToolImage,
}

impl HerdrWindow {
    /// An agent message: Markdown text with diagrams, pages, and pictures
    /// between it.
    pub(super) fn render_chat_message(
        &self,
        pane_id: &str,
        entry: EntryId,
        parts: &[Part],
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .min_w_0()
            .children(parts.iter().enumerate().map(|(index, part)| {
                // Each part scopes its own links' element ids.
                let part = match part {
                    Part::Text(lines) => self
                        .render_chat_lines(
                            "chat-agent",
                            self.chat_text_scope(pane_id, entry, Region::Body(index)),
                            lines,
                            cx,
                        )
                        .into_any_element(),
                    Part::Rich(blocks) => {
                        match self.chat_text_scope(pane_id, entry, Region::Body(index)) {
                            Some(scope) => self
                                .render_chat_rich(pane_id, blocks, visuals, &scope, cx)
                                .into_any_element(),
                            None => div().into_any_element(),
                        }
                    }
                    Part::Widget(document) => self
                        .render_chat_widget(pane_id, entry, index, document, cx)
                        .into_any_element(),
                    Part::Diagram(id, code) => self.render_chat_diagram(*id, code, visuals),
                    Part::Html { page } => self.render_chat_html(pane_id, entry, index, page, cx),
                    Part::Picture { alt } => self.render_chat_picture_caption(alt),
                    Part::Remote { id, url, alt } => self.render_chat_remote_picture(
                        pane_id, entry, index, *id, url, alt, visuals, cx,
                    ),
                };
                div().id(("chat-part", index)).min_w_0().child(part)
            }))
    }

    /// A picture the user or the agent put in the conversation.
    pub(super) fn render_chat_image(&self, author: Author, id: VisualId, visuals: &Visuals) -> Div {
        let row = div().flex().min_w_0();
        let row = match author {
            Author::User => row.justify_end(),
            Author::Agent => row,
        };
        row.child(self.render_chat_visual(id, visuals, Frame::Image))
    }

    fn render_chat_diagram(
        &self,
        id: VisualId,
        code: &[release_notes::Line],
        visuals: &Visuals,
    ) -> AnyElement {
        if let Shown::Failed(error) = visuals.shown(id) {
            // The source stays readable when it cannot be drawn.
            return div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(release_notes::render(
                    "chat-diagram",
                    code,
                    &self.theme,
                    &self.config.terminal,
                ))
                .child(
                    div()
                        .text_size(px((self.chat_text_size() - 2.).max(10.)))
                        .text_color(rgb(self.theme.muted))
                        .child(format!("Not drawn: {error}")),
                )
                .into_any_element();
        }
        self.render_chat_visual(id, visuals, Frame::Diagram)
            .into_any_element()
    }

    /// A visual at its own size, narrowed to fit, or a placeholder while it
    /// is prepared.
    pub(super) fn render_chat_visual(&self, id: VisualId, visuals: &Visuals, frame: Frame) -> Div {
        let theme = &self.theme;
        let widest = match frame {
            Frame::Diagram | Frame::Image => IMAGE_WIDTH,
            Frame::ToolImage => TOOL_IMAGE_WIDTH,
        };
        let boxed = div()
            .debug_selector(|| "chat-visual".into())
            .flex_none()
            .max_w_full()
            .overflow_hidden()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active));
        match visuals.shown(id) {
            Shown::Ready(drawn) => {
                let widest = match frame {
                    // A diagram is drawn at its natural size up to the row.
                    Frame::Diagram => drawn.width,
                    Frame::Image | Frame::ToolImage => widest,
                };
                let scale = (widest / drawn.width).min(1.);
                let (width, height) = (drawn.width * scale, drawn.height * scale);
                let picture = img(drawn.texture.clone())
                    .w(px(width))
                    .h(px(height))
                    .max_w_full()
                    .object_fit(ObjectFit::Contain);
                match frame {
                    Frame::Diagram => boxed.p(px(8.)).bg(rgb(theme.background)).child(picture),
                    Frame::Image | Frame::ToolImage => boxed.child(picture),
                }
            }
            Shown::Pending => boxed
                .w(px(widest.min(240.)))
                .h(px(PENDING_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(theme.surface))
                .text_size(self.chat_small_text())
                .text_color(rgb(theme.muted))
                .child(match frame {
                    Frame::Diagram => "Drawing diagram\u{2026}",
                    Frame::Image | Frame::ToolImage => "Loading image\u{2026}",
                }),
            Shown::Failed(error) => boxed
                .px(px(8.))
                .py(px(4.))
                .text_size(self.chat_small_text())
                .text_color(rgb(theme.muted))
                .child(format!("[image] {error}")),
        }
    }
}

//! An opened tool card's content: each item's source under its label, with
//! the exact payload a step away when the preview leaves part of it out.

use super::*;
use crate::chat::{prepared::tool::Preview, view::copy};

impl SheprdWindow {
    pub(super) fn render_chat_tool_previews(
        &self,
        pane_id: &str,
        entry: sheprd_chat_protocol::EntryId,
        summary: &ToolSummary,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        summary
            .previews
            .iter()
            .enumerate()
            .map(|(index, preview)| {
                self.render_chat_tool_preview(pane_id, entry, index, preview, visuals, cx)
                    .into_any_element()
            })
            .collect()
    }

    fn render_chat_tool_preview(
        &self,
        pane_id: &str,
        entry: sheprd_chat_protocol::EntryId,
        index: usize,
        preview: &Preview,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let small = self.chat_tool_text();
        let scope = |region| self.chat_text_scope(pane_id, entry, region);
        // Plain output already is its original; a second button would copy
        // the same text as the block's own.
        let reworked = preview.code.source != preview.original;
        let side = |text: String| {
            div()
                .text_size(small)
                .text_color(rgb(theme.muted))
                .child(text)
        };
        div()
            .id(SharedString::from(format!(
                "chat-tool-content-{}-{index}",
                entry.0
            )))
            .debug_selector(move || format!("chat-tool-content-{}-{index}", entry.0))
            .min_w_0()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .py(px(3.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child(self.render_chat_text(
                                pane_id,
                                entry,
                                Region::ToolPreviewLabel(index),
                                Cell::default(),
                                preview.label.clone(),
                                cx,
                            )),
                    )
                    .when(reworked, |row| {
                        row.child(self.rich_copy(copy::ORIGINAL, preview.original.clone(), cx))
                    }),
            )
            .when_some(preview.before.as_ref(), |card, before| {
                let region = scope(Region::ToolBefore(index));
                card.child(side(fl!("chat-diff-before")))
                    .child(div().id("before").child(self.render_chat_tool_code(
                        before,
                        region.as_ref(),
                        visuals,
                        cx,
                    )))
                    .child(side(fl!("chat-diff-after")))
            })
            .child(div().id("after").child(self.render_chat_tool_code(
                &preview.code,
                scope(Region::ToolPreview(index)).as_ref(),
                visuals,
                cx,
            )))
    }
}

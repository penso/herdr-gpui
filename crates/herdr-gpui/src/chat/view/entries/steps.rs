//! A sub-agent's own steps, listed in its call's card when it is open: one
//! quiet line each, its state's mark and its title.

use super::{Hues, tool_header};
use crate::{HerdrWindow, chat::chats::ChatPane, fonts::StyledFont};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{EntryId, EntryKind, ToolStatus};

/// `12 steps`, beside a sub-agent's state.
pub(super) fn steps_label(count: usize) -> String {
    if count == 1 {
        "1 step".to_owned()
    } else {
        format!("{count} steps")
    }
}

impl HerdrWindow {
    pub(super) fn render_chat_steps(
        &self,
        chat: &ChatPane,
        parent: EntryId,
        steps: &[EntryId],
    ) -> Div {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let transcript = chat.session.transcript();
        let text = self.chat_tool_text();
        div()
            .debug_selector(move || format!("chat-steps-{}", parent.0))
            .flex()
            .flex_col()
            .ml(px(6.))
            .pl(px(10.))
            .border_l_1()
            .border_color(rgb(theme.active))
            .text_size(text)
            .children(steps.iter().filter_map(|id| {
                let EntryKind::Tool(tool) = &transcript.entry(*id)?.kind else {
                    return None;
                };
                let (mark, color) = match tool.status {
                    ToolStatus::Pending => ("\u{25cb}", theme.muted),
                    ToolStatus::InProgress => ("\u{25cf}", hues.yellow),
                    ToolStatus::Completed => ("\u{2713}", hues.green),
                    ToolStatus::Failed => ("\u{2717}", hues.red),
                };
                let id = *id;
                Some(
                    div()
                        .debug_selector(move || format!("chat-step-{}", id.0))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().flex_none().text_color(rgb(color)).child(mark))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_font(&self.config.terminal)
                                .text_color(rgb(theme.muted))
                                .child(tool_header(&tool.title)),
                        ),
                )
            }))
    }
}

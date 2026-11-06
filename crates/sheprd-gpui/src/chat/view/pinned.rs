//! The prompt pinned over the top of the conversation once it has scrolled
//! away, as a code editor's context line keeps the enclosing function in
//! sight. It is drawn by the chat's own layout, as that prompt's row, cut to
//! a few lines. Clicking it goes back to the prompt.

use super::{
    layout::{Row, Side, layout_for},
    *,
};
use crate::notifications::safe_text;
use sheprd_chat_protocol::{Entry, EntryKind, assist::typed};

/// Lines of the prompt shown; the rest stays in the conversation.
const LINES: usize = 2;
/// Bytes of the prompt read for those lines, so a long paste costs no more
/// than a short one in each frame.
const MAX_TEXT: usize = 400;

/// The entry to pin with the list at `top`: the latest prompt while the list
/// follows the end, else the prompt of the turn being read. A prompt still
/// wholly below the list's top needs no pin.
fn pinned(entries: &[Entry], top: ListOffset, following: bool) -> Option<usize> {
    let is_prompt = |entry: &Entry| matches!(entry.kind, EntryKind::User { .. });
    let index = if following {
        entries.iter().rposition(is_prompt)?
    } else {
        let end = (top.item_ix + 1).min(entries.len());
        entries[..end].iter().rposition(is_prompt)?
    };
    let scrolled = index < top.item_ix || (index == top.item_ix && top.offset_in_item > px(0.));
    scrolled.then_some(index)
}

/// The start of `text` on running lines. It is cut before anything is copied:
/// this runs each frame the prompt is pinned, and one word can be a whole paste.
fn preview(text: &str) -> String {
    let text = text.trim_start();
    let start = &text[..text.floor_char_boundary(MAX_TEXT)];
    safe_text(
        &start.split_whitespace().collect::<Vec<_>>().join(" "),
        MAX_TEXT,
    )
}

impl SheprdWindow {
    pub(super) fn render_chat_pinned(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let entries = &chat.session.transcript().entries;
        let index = pinned(entries, chat.glide.top(), !chat.glide.stopped())?;
        let entry = entries.get(index)?;
        let EntryKind::User { text, files } = &entry.kind else {
            return None;
        };
        let theme = &self.theme;
        let look = self.chat_look();
        let layout = layout_for(self.config.chat.layout);
        let content = div()
            .debug_selector(|| "chat-pinned-text".into())
            .min_w_0()
            .text_size(px(layout.prompt_text(&look)))
            .when(!layout.mono_prose(), |text| text.text_font(&self.config.ui))
            .line_clamp(LINES)
            .child(preview(typed(text, *files)));
        let row = layout.row(
            Row {
                entry,
                previous: None,
                next: Some(Side::Agent),
                agent: chat.session.agent(),
                live: false,
                current: true,
                expanded: false,
                tool: None,
                content,
            },
            &look,
        );
        let pane = chat.target.pane_id.clone();
        Some(
            div()
                .id(SharedString::from(format!("chat-pinned-{pane}")))
                .debug_selector(|| "chat-pinned".into())
                .role(Role::Button)
                .aria_label(fl!("chat-pinned-prompt"))
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .pb(px(6.))
                .border_b_1()
                .border_color(rgb(theme.active))
                // Solid, not `with_ground`: that leaves pages showing,
                // and one scrolled under the pin would cover its text.
                .bg(rgb(theme.background))
                .occlude()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.go_to_chat_prompt(&pane, index, cx);
                }))
                .child(row)
                .into_any_element(),
        )
    }

    fn go_to_chat_prompt(&mut self, pane_id: &str, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.chats.pane_mut(pane_id) {
            pane.list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
            pane.glide.user_scrolled();
        }
        self.redraw_terminal(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;

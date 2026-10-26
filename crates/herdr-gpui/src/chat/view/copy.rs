//! Copy buttons: by an agent message, and in a rich block. Both copy only
//! text and confirm in a flash.

use super::layout::layout_for;
use crate::{HerdrWindow, config::corners, window::Flash};
use gpui::{prelude::*, *};

/// Where a message's copy button may sit, by what the message shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyPlacement {
    /// Plain prose: the layout may float the button over the message.
    Floating,
    /// Code, a table, a picture, or a control, which a floating button
    /// would cover: the layout keeps the button below the message.
    Footer,
}

/// A rich block's copy button: its name for tests, its label, and what its
/// flash says.
#[derive(Clone, Copy)]
pub(super) struct RichCopy {
    name: &'static str,
    label: &'static str,
    copied: &'static str,
}

// keep-sorted start
pub(super) const ANSWERS: RichCopy = RichCopy {
    name: "chat-copy-answers",
    label: "Copy answers",
    copied: "Copied the answers",
};
pub(super) const CODE: RichCopy = RichCopy {
    name: "chat-copy-code",
    label: "Copy code",
    copied: "Copied the code",
};
pub(super) const LATEX: RichCopy = RichCopy {
    name: "chat-copy-latex",
    label: "Copy LaTeX",
    copied: "Copied the LaTeX",
};
// keep-sorted end

impl HerdrWindow {
    /// A copy button by a message, positioned by the selected layout.
    pub(super) fn render_chat_copy(
        &self,
        id: u64,
        text: &str,
        placement: CopyPlacement,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let button = self
            .chat_copy_button(
                ("chat-copy", id),
                format!("chat-copy-{id}"),
                String::from(text),
                "Copied the message",
                cx,
            )
            .invisible()
            .group_hover("chat-agent-message", |button| button.visible())
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(4.))
            .rounded(px(corners::SMALL))
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .path("icons/copy.svg")
                    .size(px(11.))
                    .text_color(rgb(theme.muted)),
            )
            .child("Copy");
        layout_for(self.config.chat.layout)
            .copy(button, placement, &self.chat_look())
            .when(placement == CopyPlacement::Footer, |copy| {
                copy.debug_selector(move || format!("chat-copy-footer-{id}"))
            })
    }

    /// A rich block's copy button, which stays visible.
    pub(super) fn rich_copy(
        &self,
        copy: RichCopy,
        source: impl AsRef<str> + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = self.chat_look();
        let backdrop = layout_for(self.config.chat.layout).backdrop(&look);
        self.chat_copy_button(copy.name, copy.name.to_owned(), source, copy.copied, cx)
            .text_color(rgb(self.theme.subtext()))
            .px(px(6.))
            .py(px(2.))
            .rounded(px(4.))
            .when_some(backdrop, |button, fill| button.bg(rgb(fill)))
            .child(copy.label)
    }

    /// What every copy button shares: it copies `text`, says `copied` in a
    /// flash, and keeps its press from starting a text selection.
    fn chat_copy_button(
        &self,
        id: impl Into<ElementId>,
        selector: String,
        text: impl AsRef<str> + 'static,
        copied: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id)
            .debug_selector(move || selector)
            .cursor_pointer()
            .text_size(px((self.chat_text_size() - 2.).max(10.)))
            .hover(|button| button.bg(rgb(theme.active)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(text.as_ref().to_owned()));
                this.show_flash(Flash::success(copied), cx);
            }))
    }
}

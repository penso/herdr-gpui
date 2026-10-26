//! The composer's parts: the files to send, the draft and its prompt line,
//! the session's settings, context use, and the send button, which the
//! chat's layout arranges. The settings' menus open above them.

use super::{
    super::chats::ChatPane,
    Hues,
    layout::{ComposerParts, layout_for},
};
use crate::{HerdrWindow, config::corners};
use gpui::{prelude::*, *};

/// Draft lines shown above the editable one; earlier ones are counted, so a
/// long draft never pushes the editable line out of the pane.
const DRAFT_LINES: usize = 6;

impl HerdrWindow {
    pub(super) fn render_chat_composer(&self, chat: &ChatPane, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let transcript = chat.session.transcript();
        let pane_id = chat.target.pane_id.clone();
        if transcript.read_only {
            return div()
                .debug_selector(|| "chat-read-only".into())
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.))
                .p(px(12.))
                .text_size(self.chat_small_text())
                .text_color(rgb(theme.subtext()))
                .child("Continue this conversation in its terminal.")
                .child(
                    div()
                        .id("chat-reply-terminal")
                        .debug_selector(|| "chat-reply-terminal".into())
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(super::text_ink(theme.foreground, theme.surface)))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .hover(|button| button.border_color(rgb(theme.primary())))
                        .child(svg().path("icons/terminal.svg").size(px(13.)))
                        .child("Reply in terminal")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.set_chat_raw(&pane_id, true, window, cx);
                        })),
                );
        }
        let problem = chat.error.clone().or_else(|| {
            chat.session
                .rejection()
                .map(|rejection| rejection.to_string())
        });
        let small = px((self.chat_text_size() - 2.).max(10.));
        let round = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let attach = {
            let pane_id = pane_id.clone();
            round("chat-attach")
                .hover(|button| button.bg(rgb(theme.active)))
                .child(
                    svg()
                        .path("icons/plus.svg")
                        .size(px(13.))
                        .text_color(rgb(theme.subtext())),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.browse_chat_files(&pane_id, window, cx);
                }))
        };
        let copying = (chat.uploading > 0).then(|| {
            let host = match &chat.target.route {
                super::super::Route::Ssh(host) => crate::notifications::safe_text(host, 80),
                super::super::Route::Local => String::new(),
            };
            div()
                .debug_selector(|| "chat-uploading".into())
                .flex()
                .items_center()
                .px(px(8.))
                .h(px(24.))
                .rounded(px(corners::CONTROL))
                .bg(rgb(theme.active))
                .text_size(small)
                .text_color(rgb(theme.subtext()))
                .child(match chat.uploading {
                    1 => format!("Copying a file to {host}\u{2026}"),
                    count => format!("Copying {count} files to {host}\u{2026}"),
                })
        });
        let chips = (!chat.attachments.is_empty() || copying.is_some()).then(|| {
            div()
                .debug_selector(|| "chat-attachments".into())
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .px(px(10.))
                .pt(px(10.))
                .children(copying)
                .children(chat.attachments.iter().enumerate().map(|(index, path)| {
                    let name = path
                        .file_name()
                        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
                        .into_owned();
                    let picture = path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .and_then(|extension| {
                            herdr_chat_protocol::ImageType::from_mime(&format!(
                                "image/{}",
                                extension.to_ascii_lowercase()
                            ))
                        })
                        .is_some();
                    let icon = if picture {
                        "icons/zoom.svg"
                    } else {
                        "icons/file.svg"
                    };
                    let pane_id = pane_id.clone();
                    div()
                        .debug_selector(move || format!("chat-attachment-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .pl(px(8.))
                        .pr(px(2.))
                        .h(px(24.))
                        .max_w(px(260.))
                        .rounded(px(corners::CONTROL))
                        .bg(rgb(theme.active))
                        .text_size(small)
                        .child(
                            svg()
                                .flex_none()
                                .path(icon)
                                .size(px(11.))
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(div().min_w_0().truncate().child(name))
                        .child(
                            div()
                                .id(("chat-detach", index))
                                .debug_selector(move || format!("chat-detach-{index}"))
                                .flex_none()
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(corners::SMALL))
                                .cursor_pointer()
                                .hover(|button| button.bg(rgb(theme.background)))
                                .child(
                                    svg()
                                        .path("icons/close.svg")
                                        .size(px(9.))
                                        .text_color(rgb(theme.subtext())),
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.detach_chat_file(&pane_id, index, cx);
                                })),
                        )
                }))
        });
        let accepts = transcript.status.accepts_prompts();
        let send = {
            let pane_id = pane_id.clone();
            let fill = if accepts {
                theme.primary()
            } else {
                theme.active
            };
            round("chat-send")
                .bg(rgb(fill))
                .child(
                    svg()
                        .path("icons/arrow-right.svg")
                        .size(px(12.))
                        .text_color(rgb(theme.text_on(fill))),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.send_chat_composer(&pane_id, cx);
                }))
        };
        let draft = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .children((chat.draft.len() > DRAFT_LINES).then(|| {
                let earlier = chat.draft.len() - DRAFT_LINES;
                div()
                    .debug_selector(|| "chat-draft-earlier".into())
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(match earlier {
                        1 => "1 earlier line".to_owned(),
                        count => format!("{count} earlier lines"),
                    })
            }))
            .children(
                chat.draft
                    .iter()
                    .skip(chat.draft.len().saturating_sub(DRAFT_LINES))
                    .map(|line| {
                        div()
                            .text_color(rgb(theme.subtext()))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(if line.is_empty() {
                                " ".to_owned()
                            } else {
                                line.clone()
                            })
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "chat-composer".into())
                    .min_h(px(self.chat_text_size() * 1.6))
                    .flex()
                    .items_center()
                    .children(chat.composer.clone()),
            );
        let settings = self
            .render_chat_settings_buttons(chat, cx)
            .into_iter()
            .map(IntoElement::into_any_element)
            .chain(
                (!super::settings::agent_bypasses(transcript))
                    .then(|| self.render_chat_auto_accept(chat, cx).into_any_element()),
            )
            .collect();
        let parts = ComposerParts {
            chips,
            draft,
            attach,
            settings,
            usage: self.render_chat_usage(chat),
            send,
        };
        let card = layout_for(self.config.chat.layout).composer(parts, &self.chat_look());
        div()
            .relative()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(12.))
            .pt(px(8.))
            .pb(px(16.))
            .children(problem.map(|problem| {
                div()
                    .debug_selector(|| "chat-problem".into())
                    .text_size(small)
                    .text_color(rgb(hues.red))
                    .child(problem)
            }))
            .child(card)
            .children(
                chat.menu
                    .clone()
                    .map(|menu| self.render_chat_menu(chat, &menu, cx)),
            )
    }
}

//! What sits between the entries and the composer: permission requests,
//! queued prompts, and slash-command completions.

use super::{super::chats::ChatPane, Hues};
use crate::i18n::fl;
use crate::{HerdrWindow, config::corners, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Command, PermissionKind, assist};

/// Queued prompts in view at once; the list scrolls past them.
const QUEUE_ROWS: f32 = 3.5;
/// Characters of a queued prompt shown.
const QUEUED: usize = 160;

impl HerdrWindow {
    pub(super) fn render_chat_permissions(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        chat.session
            .transcript()
            .permissions
            .iter()
            .map(|permission| {
                let buttons = permission
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        let color = match option.kind {
                            kind if kind.allows() => hues.green,
                            PermissionKind::RejectOnce | PermissionKind::RejectAlways => hues.red,
                            _ => theme.foreground,
                        };
                        let pane_id = chat.target.pane_id.clone();
                        let command = Command::Answer {
                            option: option.id.clone(),
                            permission: permission.id,
                        };
                        div()
                            .id(("chat-answer", permission.id.0 * 64 + index as u64))
                            .debug_selector(move || format!("chat-answer-{index}"))
                            .flex_none()
                            .px(px(10.))
                            .py(px(3.))
                            .rounded(px(corners::CONTROL))
                            .border_1()
                            .border_color(rgb(color))
                            .text_color(rgb(color))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme.active)))
                            .child(safe_text(&option.name, 60))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.chat_send(&pane_id, command.clone(), cx);
                            }))
                    });
                div()
                    .debug_selector(|| "chat-permission".into())
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .mx(px(12.))
                    .mb(px(6.))
                    .p(px(8.))
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(hues.yellow))
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(safe_text(&permission.title, 400)),
                    )
                    .child(div().flex().flex_wrap().gap(px(6.)).children(buttons))
            })
            .collect()
    }

    pub(super) fn render_chat_queue(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let theme = &self.theme;
        let queued = &chat.session.transcript().queued;
        if queued.is_empty() {
            return None;
        }
        let rows: Vec<Div> = queued
            .iter()
            .enumerate()
            .map(|(index, queued)| {
                let pane_id = chat.target.pane_id.clone();
                let id = queued.id;
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .mx(px(12.))
                    .mb(px(4.))
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(corners::SMALL))
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.muted))
                            .child(fl!("chat-queued")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(safe_text(&queued.text, QUEUED)),
                    )
                    .child(
                        div()
                            .id(("chat-unqueue", index))
                            .debug_selector(move || format!("chat-unqueue-{index}"))
                            .flex_none()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(corners::SMALL))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme.active)))
                            .child(
                                svg()
                                    .path("icons/close.svg")
                                    .size(px(10.))
                                    .text_color(rgb(theme.foreground)),
                            )
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.chat_send(&pane_id, Command::Unqueue { id }, cx);
                            })),
                    )
            })
            .collect();
        // A height-limited list that scrolls, so a long queue keeps every
        // prompt's withdraw button and never pushes the composer out.
        let row = self.chat_text_size() * 1.5 + 10.;
        Some(
            div()
                .id("chat-queue")
                .debug_selector(|| "chat-queue".into())
                .flex_none()
                .flex()
                .flex_col()
                .max_h(px(row * QUEUE_ROWS))
                .overflow_y_scroll()
                .children(rows),
        )
    }

    /// The agent's slash commands the composer's text would complete to,
    /// over the composer; Tab or a click takes the selected one.
    pub(super) fn render_chat_completions(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let theme = &self.theme;
        let text = chat.composer.as_ref()?.read(cx).text().to_owned();
        let found = assist::completions(chat.session.transcript(), &text);
        if found.is_empty() {
            return None;
        }
        let selected = chat.completion.min(found.len() - 1);
        let rows = found.into_iter().enumerate().map(|(index, command)| {
            let pane_id = chat.target.pane_id.clone();
            let name = command.name.clone();
            div()
                .id(("chat-completion", index))
                .debug_selector(move || format!("chat-completion-{index}"))
                .flex()
                .gap(px(8.))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .when(index == selected, |row| row.bg(rgb(theme.active)))
                .hover(|row| row.bg(rgb(theme.active)))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("/{}", safe_text(&command.name, 60))),
                )
                .children(command.hint.as_ref().map(|hint| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(safe_text(hint, 60))
                }))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.subtext()))
                        .child(safe_text(&command.description, 200)),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.complete_chat_command(&pane_id, &name, window, cx);
                }))
        });
        Some(
            div()
                .debug_selector(|| "chat-completions".into())
                .flex_none()
                .flex()
                .flex_col()
                .mx(px(12.))
                .mb(px(4.))
                .p(px(4.))
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.surface))
                .children(rows),
        )
    }
}

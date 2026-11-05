//! What an empty chat shows above its composer: the modes it can run in,
//! the current one marked, so the user sees what a new chat carried over
//! from their last choice and can change it before the first prompt. A
//! pick here goes through the same send as the composer's buttons, so the
//! next new chat follows it too.

use super::{super::chats::ChatPane, ChatInput, Hues, settings};
use crate::{SheprdWindow, config::corners, i18n::fl, motion, notifications::safe_text};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{Command, OptionCategory, OptionKind, OptionValue, Transcript};

const WIDTH: f32 = 440.;

/// One mode an empty chat offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::chat) struct ModeRow {
    /// Whether it runs without asking, which the row warns of.
    pub(in crate::chat) bypass: bool,
    pub(in crate::chat) command: Command,
    pub(in crate::chat) current: bool,
    pub(in crate::chat) description: Option<String>,
    pub(in crate::chat) name: String,
}

/// The agent's modes: its mode setting when it has one, else its older list.
pub(in crate::chat) fn mode_rows(transcript: &Transcript) -> Vec<ModeRow> {
    if let Some(option) = transcript.option(OptionCategory::Mode) {
        let OptionKind::Select { choices, current } = &option.kind else {
            return Vec::new();
        };
        return choices
            .iter()
            .map(|choice| ModeRow {
                bypass: settings::bypasses(&choice.id, &choice.name),
                command: Command::SetOption {
                    id: option.id.clone(),
                    value: OptionValue::Choice {
                        id: choice.id.clone(),
                    },
                },
                current: choice.id == *current,
                description: choice.description.clone(),
                name: choice.name.clone(),
            })
            .collect();
    }
    transcript.modes.as_ref().map_or_else(Vec::new, |modes| {
        modes
            .available
            .iter()
            .map(|mode| ModeRow {
                bypass: settings::bypasses(&mode.id, &mode.name),
                command: Command::SetMode {
                    id: mode.id.clone(),
                },
                current: mode.id == modes.current,
                description: mode.description.clone(),
                name: mode.name.clone(),
            })
            .collect()
    })
}

impl SheprdWindow {
    /// The panel over an empty chat, or nothing once a message shows, for a
    /// chat typed into elsewhere, or an agent with no modes to pick.
    pub(super) fn render_chat_opening(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let transcript = chat.session.transcript();
        if !transcript.entries.is_empty() || chat.input() != ChatInput::Composer {
            return None;
        }
        let rows = mode_rows(transcript);
        if rows.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let yellow = Hues::of(theme).yellow;
        let agent = safe_text(chat.session.agent(), 80);
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let accent = if row.bypass { yellow } else { theme.primary() };
                let pane = chat.target.pane_id.clone();
                let command = row.command;
                div()
                    .id(("chat-opening-mode", index))
                    .debug_selector(move || format!("chat-opening-mode-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(if row.current { accent } else { theme.active }))
                    .when(row.current, |row| {
                        row.bg(rgb(crate::config::mix(theme.background, accent, 14)))
                    })
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.active)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.chat_send(&pane, command.clone(), cx);
                    }))
                    .children(row.bypass.then(|| {
                        svg()
                            .flex_none()
                            .path("icons/shield.svg")
                            .size(px(13.))
                            .text_color(rgb(yellow))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .when(row.current, |name| name.font_weight(FontWeight::MEDIUM))
                                    .child(safe_text(&row.name, 80)),
                            )
                            .children(row.description.as_ref().map(|description| {
                                div()
                                    .text_size(self.chat_small_text())
                                    .text_color(rgb(theme.subtext()))
                                    .child(safe_text(description, 200))
                            })),
                    )
            })
            .collect::<Vec<_>>();
        let auto_accept = (!settings::agent_bypasses(transcript))
            .then(|| self.render_chat_auto_accept("chat-opening-auto-accept", chat, cx));
        let panel = div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.))
            .child(
                div()
                    .id("chat-opening")
                    .debug_selector(|| "chat-opening".into())
                    .w_full()
                    .max_w(px(WIDTH))
                    .max_h_full()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(fl!("chat-opening-title", agent = agent.clone())),
                    )
                    .child(
                        div()
                            .pb(px(4.))
                            .text_size(self.chat_small_text())
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("chat-opening-hint", agent = agent)),
                    )
                    .children(rows)
                    .children(auto_accept.map(|button| div().pt(px(4.)).flex().child(button))),
            );
        Some(motion::pop_in(panel, "chat-opening-in").into_any_element())
    }
}

//! The session's settings along the composer's foot: a button per setting worth
//! having at hand (model, thinking level, mode), the menu each opens, the
//! rest behind one more button, and how full the context window is.

use super::{super::chats::ChatPane, Hues};
use crate::{
    HerdrWindow, chat::chats::Menu, config::corners, notifications::safe_text,
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{
    Choice, Command, ConfigOption, OptionCategory, OptionKind, OptionValue, Transcript, Usage,
    display,
};

/// The settings the header shows a button for, in order.
const AT_HAND: [OptionCategory; 3] = [
    OptionCategory::Model,
    OptionCategory::ThoughtLevel,
    OptionCategory::Mode,
];
/// The share of the context window past which the meter warns, then alarms.
const WARN: u8 = 75;
const ALARM: u8 = 90;
const METER: f32 = 48.;
/// A setting with this many choices, other than the model, is a switch
/// between them rather than a menu: build or plan, say.
const SWITCH: usize = 2;
/// Menus with more choices than this open with a search field.
const SEARCHABLE: usize = 8;

/// The choices whose name, group, or description holds `query`, ignoring
/// case, with their place in the full list.
pub(in crate::chat) fn matching<'a>(
    choices: &'a [Choice],
    query: &str,
) -> impl Iterator<Item = (usize, &'a Choice)> {
    let query = query.trim().to_lowercase();
    choices.iter().enumerate().filter(move |(_, choice)| {
        query.is_empty()
            || [
                Some(&choice.name),
                choice.group.as_ref(),
                choice.description.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.to_lowercase().contains(&query))
    })
}

/// Whether the agent's own modes already include not asking for
/// permission, such as Claude's "Bypass permissions"; the composer then
/// leaves out its auto-accept button rather than offer the same twice.
pub(super) fn agent_bypasses(transcript: &Transcript) -> bool {
    let bypass = |id: &str, name: &str| {
        [id, name]
            .iter()
            .any(|text| text.to_ascii_lowercase().contains("bypass"))
    };
    let options = transcript
        .options
        .iter()
        .filter(|option| option.category == OptionCategory::Mode)
        .any(|option| match &option.kind {
            OptionKind::Select { choices, .. } => choices
                .iter()
                .any(|choice| bypass(&choice.id, &choice.name)),
            OptionKind::Toggle { .. } => false,
        });
    let modes = transcript.modes.as_ref().is_some_and(|modes| {
        modes
            .available
            .iter()
            .any(|mode| bypass(&mode.id, &mode.name))
    });
    options || modes
}

/// Whether `option` shows as a switch.
fn switches(option: &ConfigOption) -> bool {
    matches!(&option.kind, OptionKind::Select { choices, .. } if choices.len() == SWITCH)
        && option.category != OptionCategory::Model
}

/// The settings the header has no button for.
fn others(transcript: &Transcript) -> Vec<&ConfigOption> {
    transcript
        .options
        .iter()
        .filter(|option| {
            !AT_HAND.contains(&option.category)
                || transcript.option(option.category).map(|first| &first.id) != Some(&option.id)
        })
        .collect()
}

impl HerdrWindow {
    fn chat_header_button(&self, id: SharedString) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(8.))
            .h(px(26.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.active))
            .text_size(self.chat_small_text())
            .cursor_pointer()
            .hover(|button| button.bg(rgb(crate::config::mix(theme.active, theme.foreground, 12))))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    /// Opens `menu`, or closes it when it is open. A long setting menu
    /// opens with its search field holding the keyboard; closing it gives
    /// the keyboard back to the composer.
    pub(in crate::chat) fn toggle_chat_menu(
        &mut self,
        pane_id: &str,
        menu: Menu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let font = self.chat_font();
        let theme = self.theme.clone();
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let opening = pane.menu.as_ref() != Some(&menu);
        let long = match &menu {
            Menu::Setting(id) => pane
                .session
                .transcript()
                .options
                .iter()
                .find(|option| &option.id == id)
                .is_some_and(|option| {
                    matches!(&option.kind, OptionKind::Select { choices, .. }
                        if choices.len() > SEARCHABLE)
                }),
            Menu::Modes | Menu::Settings => false,
        };
        pane.menu = opening.then_some(menu);
        let search = (opening && long).then(|| {
            let input = cx.new(|cx| {
                let mut input = SearchInput::new(cx);
                // The menu draws the field's frame.
                input.set_frameless(cx);
                input.set_appearance(font, theme, cx);
                input.set_placeholder("Search\u{2026}", cx);
                input
            });
            let redraw = cx.observe(&input, |this, _, cx| this.redraw_terminal(cx));
            (input, redraw)
        });
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        pane.menu_search = search;
        let focus = pane
            .menu_search
            .as_ref()
            .map(|(input, _)| input)
            .or(pane.composer.as_ref())
            .map(|input| input.read(cx).focus.clone());
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// An on/off switch between a setting's two choices, labeled with the
    /// current one: off is the first choice, on the second.
    fn chat_switch(
        &self,
        id: String,
        choices: Vec<(String, bool, Command)>,
        pane_id: &str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let on = choices.get(1).is_some_and(|(_, current, _)| *current);
        let name = choices
            .iter()
            .find(|(_, current, _)| *current)
            .map(|(name, _, _)| name.clone())
            .unwrap_or_default();
        let other = choices
            .into_iter()
            .find(|(_, current, _)| !*current)
            .map(|(_, _, command)| command);
        let pane = pane_id.to_owned();
        let fill = if on { theme.primary() } else { theme.active };
        let selector = id.clone();
        div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(4.))
            .h(px(26.))
            .cursor_pointer()
            .text_size(px((self.chat_text_size() - 2.).max(10.)))
            .text_color(rgb(if on { theme.primary() } else { theme.subtext() }))
            .child(
                div()
                    .flex_none()
                    .w(px(26.))
                    .h(px(15.))
                    .p(px(2.))
                    .rounded_full()
                    .bg(rgb(fill))
                    .flex()
                    .when(on, |track| track.justify_end())
                    .child(div().size(px(11.)).rounded_full().bg(rgb(theme.background))),
            )
            .child(safe_text(&name, 40))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if let Some(command) = other.clone() {
                    this.chat_send(&pane, command, cx);
                }
            }))
    }

    /// Whether permission requests are answered without asking: a shield
    /// that turns yellow while they are.
    pub(super) fn render_chat_auto_accept(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let yellow = Hues::of(theme).yellow;
        let on = chat.session.transcript().auto_accept;
        let pane = chat.target.pane_id.clone();
        let color = if on { yellow } else { theme.subtext() };
        div()
            .id("chat-auto-accept")
            .debug_selector(|| "chat-auto-accept".into())
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(8.))
            .h(px(26.))
            .rounded(px(corners::CONTROL))
            .cursor_pointer()
            .border_1()
            .border_color(rgb(if on { yellow } else { theme.active }))
            .bg(rgb(if on {
                crate::config::mix(theme.background, yellow, 18)
            } else {
                theme.background
            }))
            .text_size(px((self.chat_text_size() - 2.).max(10.)))
            .text_color(rgb(color))
            .child(
                svg()
                    .flex_none()
                    .path("icons/shield.svg")
                    .size(px(12.))
                    .text_color(rgb(color)),
            )
            .child(if on { "Auto-accepting" } else { "Asks first" })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.chat_send(&pane, Command::SetAutoAccept { on: !on }, cx);
            }))
    }

    /// The composer's setting buttons: one per setting at hand, the agent's
    /// older modes when it has no mode setting, then one for the rest.
    pub(super) fn render_chat_settings_buttons(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let theme = &self.theme;
        let transcript = chat.session.transcript();
        let pane_id = chat.target.pane_id.clone();
        let chevron = || {
            svg()
                .path("icons/chevron-down.svg")
                .size(px(10.))
                .text_color(rgb(theme.subtext()))
        };
        let mut buttons = Vec::new();
        for category in AT_HAND {
            let Some(option) = transcript.option(category) else {
                continue;
            };
            if switches(option)
                && let OptionKind::Select { choices, current } = &option.kind
            {
                let switch = choices
                    .iter()
                    .map(|choice| {
                        let command = Command::SetOption {
                            id: option.id.clone(),
                            value: OptionValue::Choice {
                                id: choice.id.clone(),
                            },
                        };
                        (choice.name.clone(), &choice.id == current, command)
                    })
                    .collect();
                buttons.push(self.chat_switch(
                    format!("chat-setting-{}", option.id),
                    switch,
                    &pane_id,
                    cx,
                ));
                continue;
            }
            let id = option.id.clone();
            let pane = pane_id.clone();
            let button = self
                .chat_header_button(format!("chat-setting-{}", option.id).into())
                .child(
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(safe_text(&option.name, 40)),
                )
                .child(
                    div()
                        .max_w(px(200.))
                        .truncate()
                        .child(safe_text(&option.current_name(), 80)),
                );
            buttons.push(match &option.kind {
                OptionKind::Toggle { current } => {
                    let value = OptionValue::Toggle { value: !current };
                    button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        let command = Command::SetOption {
                            id: id.clone(),
                            value: value.clone(),
                        };
                        this.chat_send(&pane, command, cx);
                    }))
                }
                OptionKind::Select { .. } => button.child(chevron()).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.toggle_chat_menu(&pane, Menu::Setting(id.clone()), window, cx);
                    },
                )),
            });
        }
        if transcript.option(OptionCategory::Mode).is_none()
            && let Some(modes) = &transcript.modes
        {
            let name = modes
                .current()
                .map_or_else(|| modes.current.clone(), |mode| mode.name.clone());
            let pane = pane_id.clone();
            if modes.available.len() == SWITCH {
                let switch = modes
                    .available
                    .iter()
                    .map(|mode| {
                        let command = Command::SetMode {
                            id: mode.id.clone(),
                        };
                        (mode.name.clone(), mode.id == modes.current, command)
                    })
                    .collect();
                buttons.push(self.chat_switch("chat-mode".into(), switch, &pane, cx));
            } else {
                buttons.push(
                    self.chat_header_button("chat-mode".into())
                        .child(div().max_w(px(160.)).truncate().child(safe_text(&name, 80)))
                        .child(chevron())
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.toggle_chat_menu(&pane, Menu::Modes, window, cx);
                        })),
                );
            }
        }
        if !others(transcript).is_empty() {
            let pane = pane_id.clone();
            buttons.push(
                self.chat_header_button("chat-more-settings".into())
                    .child(
                        svg()
                            .path("icons/settings.svg")
                            .size(px(12.))
                            .text_color(rgb(theme.foreground)),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.toggle_chat_menu(&pane, Menu::Settings, window, cx);
                    })),
            );
        }
        buttons
    }

    /// How full the context window is.
    pub(super) fn render_chat_usage(&self, chat: &ChatPane) -> Option<Div> {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let usage = chat
            .session
            .transcript()
            .usage
            .as_ref()
            .filter(|usage| usage.size > 0)?;
        let percent = usage.percent();
        let color = match percent {
            ALARM.. => hues.red,
            WARN.. => hues.yellow,
            _ => hues.green,
        };
        let filled = METER * f32::from(percent) / 100.;
        Some(
            div()
                .debug_selector(|| "chat-usage".into())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(rgb(theme.subtext()))
                .child(
                    div()
                        .w(px(METER))
                        .h(px(4.))
                        .rounded_full()
                        .bg(rgb(theme.active))
                        .child(div().w(px(filled)).h_full().rounded_full().bg(rgb(color))),
                )
                .child(usage_label(usage)),
        )
    }

    /// The menu `menu` opens, over the top of the chat.
    pub(super) fn render_chat_menu(
        &self,
        chat: &ChatPane,
        menu: &Menu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let transcript = chat.session.transcript();
        let pane_id = chat.target.pane_id.clone();
        let row = |index: usize, current: bool| {
            div()
                .id(("chat-menu-row", index))
                .debug_selector(move || format!("chat-menu-row-{index}"))
                .flex()
                .flex_col()
                .px(px(8.))
                .py(px(4.))
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .when(current, |row| row.bg(rgb(theme.active)))
                .hover(|row| row.bg(rgb(theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let heading = |text: &str| {
            div()
                .px(px(8.))
                .pt(px(6.))
                .pb(px(2.))
                .text_size(self.chat_small_text())
                .text_color(rgb(theme.muted))
                .child(safe_text(text, 80))
        };
        let described = |name: &str, description: Option<&String>| {
            (
                div().child(safe_text(name, 80)),
                description.map(|description| {
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(safe_text(description, 200))
                }),
            )
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        match menu {
            Menu::Modes => {
                let Some(modes) = &transcript.modes else {
                    return div().into_any_element();
                };
                for (index, mode) in modes.available.iter().enumerate() {
                    let (name, description) = described(&mode.name, mode.description.as_ref());
                    let pane = pane_id.clone();
                    let command = Command::SetMode {
                        id: mode.id.clone(),
                    };
                    rows.push(
                        row(index, mode.id == modes.current)
                            .child(name)
                            .children(description)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                this.toggle_chat_menu(&pane, Menu::Modes, window, cx);
                                this.chat_send(&pane, command.clone(), cx);
                            }))
                            .into_any_element(),
                    );
                }
            }
            Menu::Setting(id) => {
                let Some(option) = transcript.options.iter().find(|option| &option.id == id) else {
                    return div().into_any_element();
                };
                let OptionKind::Select { choices, current } = &option.kind else {
                    return div().into_any_element();
                };
                rows.push(heading(&option.name).into_any_element());
                let query = chat
                    .menu_search
                    .as_ref()
                    .map(|(search, _)| search.read(cx).text().to_owned())
                    .unwrap_or_default();
                if let Some(search) = chat.menu_search.as_ref().map(|(search, _)| search.clone()) {
                    rows.push(
                        div()
                            .debug_selector(|| "chat-menu-search".into())
                            .mx(px(4.))
                            .mb(px(4.))
                            .px(px(6.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .rounded(px(corners::SMALL))
                            .border_1()
                            .border_color(rgb(theme.active))
                            .bg(rgb(theme.background))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(search)
                            .into_any_element(),
                    );
                }
                let mut found = matching(choices, &query).peekable();
                if found.peek().is_none() {
                    rows.push(
                        div()
                            .px(px(8.))
                            .py(px(4.))
                            .text_color(rgb(theme.muted))
                            .child("No match")
                            .into_any_element(),
                    );
                }
                let mut group = None;
                for (index, choice) in found {
                    if choice.group.is_some() && choice.group != group {
                        group.clone_from(&choice.group);
                        rows.push(
                            heading(choice.group.as_deref().unwrap_or("")).into_any_element(),
                        );
                    }
                    let (name, description) = described(&choice.name, choice.description.as_ref());
                    let pane = pane_id.clone();
                    let menu = menu.clone();
                    let command = Command::SetOption {
                        id: option.id.clone(),
                        value: OptionValue::Choice {
                            id: choice.id.clone(),
                        },
                    };
                    rows.push(
                        row(index, &choice.id == current)
                            .child(name)
                            .children(description)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                this.toggle_chat_menu(&pane, menu.clone(), window, cx);
                                this.chat_send(&pane, command.clone(), cx);
                            }))
                            .into_any_element(),
                    );
                }
            }
            Menu::Settings => {
                for (index, option) in others(transcript).into_iter().enumerate() {
                    let pane = pane_id.clone();
                    let id = option.id.clone();
                    let value = match &option.kind {
                        OptionKind::Toggle { current } => {
                            Some(OptionValue::Toggle { value: !current })
                        }
                        OptionKind::Select { .. } => None,
                    };
                    rows.push(
                        row(index, false)
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(div().flex_1().child(safe_text(&option.name, 80)))
                                    .child(
                                        div()
                                            .text_color(rgb(theme.subtext()))
                                            .child(safe_text(&option.current_name(), 80)),
                                    ),
                            )
                            .children(option.description.as_ref().map(|description| {
                                div()
                                    .text_color(rgb(theme.subtext()))
                                    .child(safe_text(description, 200))
                            }))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                match &value {
                                    Some(value) => {
                                        let command = Command::SetOption {
                                            id: id.clone(),
                                            value: value.clone(),
                                        };
                                        this.chat_send(&pane, command, cx);
                                    }
                                    None => this.toggle_chat_menu(
                                        &pane,
                                        Menu::Setting(id.clone()),
                                        window,
                                        cx,
                                    ),
                                }
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        // Over the composer, whose foot holds the buttons that open it.
        div()
            .absolute()
            .bottom(relative(1.))
            .left(px(12.))
            .w(px(360.))
            .child(
                div()
                    .id("chat-menu")
                    .debug_selector(|| "chat-menu".into())
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .p(px(4.))
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.surface))
                    .shadow_md()
                    .children(rows),
            )
            .into_any_element()
    }
}

/// The context meter's label: tokens used of the window. What the session
/// has cost stays out of the composer.
fn usage_label(usage: &Usage) -> String {
    format!(
        "{} / {}",
        display::tokens(usage.used),
        display::tokens(usage.size)
    )
}

#[cfg(test)]
mod tests;

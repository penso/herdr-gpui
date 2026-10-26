//! Native controls for a validated `herdr-ui` card. Editing only changes
//! local form state. Actions use the ordinary composer, after a user click.

use super::super::{
    Target,
    chats::ChatInput,
    widgets::{ActionMode, Component, Document, Form, Input, Key, Value, bounded_input},
};
use crate::{
    HerdrWindow,
    config::{Theme, corners},
    search_input::{Changed, SearchInput},
};
use components::{Control, SliderRange};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Cursor, EntryId};
use std::rc::Rc;

mod components;

#[derive(Clone)]
struct Request {
    target: Target,
    cursor: Option<Cursor>,
    entry: EntryId,
    block: usize,
    document: Rc<Document>,
}

impl Request {
    fn key(&self) -> Key {
        (self.entry, self.block)
    }

    /// A callback from an old frame cannot edit or send into a replacement
    /// session, a replaced message, or a removed card.
    fn live(&self, view: &HerdrWindow) -> bool {
        view.chats.pane(&self.target.pane_id).is_some_and(|pane| {
            pane.is_current(&self.target, self.cursor)
                && pane.prepared.shows_widget(self.key(), &self.document)
        })
    }
}

#[derive(Clone)]
struct SliderDrag(String);

fn button(theme: &Theme, id: String, label: String, selected: bool) -> Stateful<Div> {
    let fill = if selected {
        theme.primary()
    } else {
        theme.surface
    };
    let text = super::text_ink(
        if selected {
            theme.text_on(fill)
        } else {
            theme.foreground
        },
        fill,
    );
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .role(Role::Button)
        .aria_label(label.clone())
        .px(px(12.))
        .py(px(7.))
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(rgb(if selected { fill } else { theme.active }))
        .bg(rgb(fill))
        .text_color(rgb(text))
        .cursor_pointer()
        .hover(move |button| button.border_color(rgb(text)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(label)
}

fn label(theme: &Theme, text: &str) -> Div {
    div()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(theme.subtext()))
        .child(text.to_owned())
}

fn field(theme: &Theme, title: &str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .min_w_0()
        .child(label(theme, title))
        .child(control)
}

/// One card's controls: what their edits carry, the card's local form once
/// it is edited, and what takes the chat's prompts, which actions need.
struct Card<'a> {
    request: Request,
    form: Option<&'a Form>,
    input: ChatInput,
}

impl Card<'_> {
    /// A control's current value: its edit, or the agent's default.
    fn value(&self, component: &Component) -> Option<Value> {
        component.field().map(|(id, default)| {
            self.form
                .and_then(|form| form.values.get(id))
                .cloned()
                .unwrap_or(default)
        })
    }
}

impl HerdrWindow {
    pub(in crate::chat) fn render_chat_widget(
        &self,
        pane_id: &str,
        entry: EntryId,
        block: usize,
        document: &Rc<Document>,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(pane) = self.chats.pane(pane_id) else {
            return div();
        };
        let request = Request {
            target: pane.target.clone(),
            cursor: pane.session.since(),
            entry,
            block,
            document: document.clone(),
        };
        let card = Card {
            form: pane.widgets.form(request.key(), document),
            input: pane.input(),
            request,
        };
        let prefix = format!("widget-{pane_id}-{}-{block}", entry.0);
        let parts = document
            .components
            .iter()
            .enumerate()
            .map(|(index, component)| {
                let id = format!("{prefix}-{index}");
                self.render_widget_component(&card, id, component, cx)
            })
            .collect::<Vec<_>>();
        let theme = &self.theme;
        div()
            .debug_selector(|| "chat-widget".into())
            .flex()
            .flex_col()
            .gap(px(14.))
            .min_w_0()
            .p(px(16.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .child(self.render_widget_header(&document.title))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .min_w_0()
                    .children(parts),
            )
    }

    fn render_widget_header(&self, title: &str) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(super::text_ink(theme.primary(), theme.surface)))
                    .child("INTERACTIVE"),
            )
            .when(!title.is_empty(), |header| {
                header.child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_owned()),
                )
            })
    }

    fn render_widget_component(
        &self,
        card: &Card<'_>,
        id: String,
        component: &Component,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = card.value(component);
        match component {
            Component::Text { text } => div().min_w_0().child(text.clone()),
            Component::Card { title, text } => self.widget_note(title, text),
            Component::Metric {
                label,
                value,
                detail,
            } => self.widget_metric(label, value, detail),
            Component::Choice {
                id: name,
                label,
                options,
                ..
            } => self.widget_choice(card, Control { id, name, label }, options, value, cx),
            Component::Toggle {
                id: name, label, ..
            } => {
                let on = matches!(value, Some(Value::Toggle(true)));
                self.widget_toggle(card, Control { id, name, label }, on, cx)
            }
            Component::Slider {
                id: name,
                label,
                min,
                max,
                step,
                value: initial,
            } => {
                let current = match value {
                    Some(Value::Number(value)) => value,
                    _ => *initial,
                };
                let range = SliderRange {
                    min: *min,
                    max: *max,
                    step: *step,
                };
                self.widget_slider(
                    &card.request,
                    Control { id, name, label },
                    current,
                    range,
                    cx,
                )
            }
            Component::Input {
                id: name,
                label,
                placeholder,
                ..
            } => {
                let shown = match value {
                    Some(Value::Text(value)) => value,
                    _ => String::new(),
                };
                self.widget_input(card, Control { id, name, label }, placeholder, shown, cx)
            }
            Component::Table { columns, rows } => self.widget_table(columns, rows),
            Component::Chart { label, points } => self.widget_chart(label, points),
            Component::Action {
                label,
                prompt,
                mode,
            } => self.widget_action_button(card, id, label, prompt, *mode, cx),
        }
    }

    fn change_widget(&mut self, request: &Request, id: &str, value: Value, cx: &mut Context<Self>) {
        if !request.live(self) {
            return;
        }
        if let Some(pane) = self.chats.pane_mut(&request.target.pane_id) {
            let form = pane.widgets.edit(request.key(), &request.document);
            if request.document.set(&mut form.values, id, value) {
                pane.remeasure(request.entry);
                cx.notify();
            }
        }
    }

    fn edit_widget_input(
        &mut self,
        request: &Request,
        id: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !request.live(self) {
            return;
        }
        let Some(pane) = self.chats.pane_mut(&request.target.pane_id) else {
            return;
        };
        let form = pane.widgets.edit(request.key(), &request.document);
        if let Some(input) = form.inputs.get(id) {
            let focus = input.entity.read(cx).focus.clone();
            window.focus(&focus, cx);
            return;
        }
        let value = match form.values.get(id) {
            Some(Value::Text(value)) => value.clone(),
            _ => return,
        };
        let input = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_frameless(cx);
            input.set_placeholder(placeholder, cx);
            input.set_text_at(&value, value.len(), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            input
        });
        let request_changed = request.clone();
        let name = id.to_owned();
        let changed = cx.subscribe(&input, move |this, input, _: &Changed, cx| {
            if input.read(cx).is_composing() {
                return;
            }
            let value = bounded_input(input.read(cx).text());
            if value != input.read(cx).text() {
                input.update(cx, |input, cx| input.set_text_at(&value, value.len(), cx));
            }
            this.change_widget(&request_changed, &name, Value::Text(value), cx);
        });
        let focus = input.read(cx).focus.clone();
        form.inputs.insert(
            id.to_owned(),
            Input {
                entity: input,
                _changed: changed,
            },
        );
        pane.remeasure(request.entry);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn widget_action(
        &mut self,
        request: &Request,
        prefix: &str,
        mode: ActionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !request.live(self) {
            return;
        }
        let Some(pane) = self.chats.pane_mut(&request.target.pane_id) else {
            return;
        };
        if pane.input() != ChatInput::Composer {
            return;
        }
        let form = pane.widgets.edit(request.key(), &request.document);
        if form
            .inputs
            .values()
            .any(|input| input.entity.read(cx).is_composing())
        {
            return;
        }
        let prompt = match request.document.prompt(prefix, &form.values) {
            Ok(prompt) => prompt,
            Err(error) => {
                pane.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let Some(composer) = pane.composer.clone() else {
            return;
        };
        if mode == ActionMode::Send
            && (!pane.attached() || !pane.session.transcript().status.accepts_prompts())
        {
            pane.error = Some("The agent is not ready to receive this form".into());
            cx.notify();
            return;
        }
        let existing = composer.read(cx).text().to_owned();
        if mode == ActionMode::Send
            && (!existing.is_empty()
                || !pane.draft.is_empty()
                || !pane.attachments.is_empty()
                || pane.uploading > 0)
        {
            pane.error = Some("Finish your current message before sending this form".into());
            cx.notify();
            return;
        }
        if !existing.is_empty() {
            pane.draft.push(existing);
            pane.draft.push(String::new());
        }
        let mut lines: Vec<String> = prompt.lines().map(str::to_owned).collect();
        let last = lines.pop().unwrap_or_default();
        pane.draft.extend(lines);
        pane.error = None;
        composer.update(cx, |input, cx| input.set_text_at(&last, last.len(), cx));
        let focus = composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        if mode == ActionMode::Send {
            self.send_chat_composer(&request.target.pane_id, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;

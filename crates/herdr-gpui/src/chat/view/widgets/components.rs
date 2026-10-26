//! One builder per component of a `herdr-ui` card. Each draws from the
//! card's local form and edits only that.

use super::{Card, Request, SliderDrag, button, field, label};
use crate::{
    HerdrWindow,
    chat::ChatInput,
    chat::widgets::{ActionMode, Point, Value, slider_value, snap},
    config::corners,
};
use gpui::{prelude::*, *};

/// A control that edits one value of the card's form: its element id, the
/// value's name in the form, and its label.
pub(super) struct Control<'a> {
    pub(super) id: String,
    pub(super) name: &'a str,
    pub(super) label: &'a str,
}

/// Where a slider's value may go.
#[derive(Clone, Copy)]
pub(super) struct SliderRange {
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) step: f64,
}

impl HerdrWindow {
    pub(super) fn widget_note(&self, title: &str, text: &str) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(12.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.background))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(text.to_owned()),
            )
    }

    pub(super) fn widget_metric(&self, title: &str, value: &str, detail: &str) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(label(theme, title))
            .child(
                div()
                    .text_size(px(self.chat_text_size() * 1.8))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(super::super::text_ink(theme.primary(), theme.surface)))
                    .child(value.to_owned()),
            )
            .when(!detail.is_empty(), |metric| {
                metric.child(label(theme, detail))
            })
    }

    pub(super) fn widget_choice(
        &self,
        card: &Card<'_>,
        control: Control<'_>,
        options: &[String],
        selected: Option<Value>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let choices =
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .children(options.iter().enumerate().map(|(index, option)| {
                    let request = card.request.clone();
                    let name = control.name.to_owned();
                    let value = Value::Text(option.clone());
                    button(
                        theme,
                        format!("{}-{index}", control.id),
                        option.clone(),
                        selected.as_ref() == Some(&value),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.change_widget(&request, &name, value.clone(), cx);
                    }))
                }));
        field(theme, control.label, choices)
    }

    /// The same switch Settings uses, which flips the value on a click.
    pub(super) fn widget_toggle(
        &self,
        card: &Card<'_>,
        control: Control<'_>,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let request = card.request.clone();
        let name = control.name.to_owned();
        let id = control.id;
        let switch = crate::toggles::switch(theme, 20., on)
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id)
            .role(Role::Switch)
            .aria_label(control.label.to_owned())
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.change_widget(&request, &name, Value::Toggle(!on), cx);
            }));
        field(theme, control.label, switch)
    }

    pub(super) fn widget_slider(
        &self,
        request: &Request,
        control: Control<'_>,
        value: f64,
        range: SliderRange,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let SliderRange { min, max, step } = range;
        let Control { id, name, .. } = &control;
        let fraction = ((value - min) / (max - min)).clamp(0., 1.) as f32;
        let drag_id = id.clone();
        let target_id = id.clone();
        let request_drag = request.clone();
        let field_name = (*name).to_owned();
        let track = div()
            .id(SharedString::from(format!("{id}-track")))
            .flex_1()
            .min_w(px(60.))
            .h(px(24.))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .bg(rgb(theme.active))
                    .child(
                        div()
                            .w(relative(fraction))
                            .h_full()
                            .rounded_full()
                            .bg(rgb(theme.primary())),
                    ),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_drag(SliderDrag(drag_id), |_, _, _, cx| cx.new(|_| Empty))
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<SliderDrag>, _, cx| {
                    if event.drag(cx).0 != target_id {
                        return;
                    }
                    let width = f32::from(event.bounds.size.width).max(1.);
                    let fraction = f64::from(
                        (f32::from(event.event.position.x) - f32::from(event.bounds.left()))
                            / width,
                    );
                    this.change_widget(
                        &request_drag,
                        &field_name,
                        Value::Number(slider_value(fraction, min, max, step)),
                        cx,
                    );
                }),
            );
        let stepper = |suffix: &str, glyph: &str, delta: f64, cx: &mut Context<Self>| {
            let request = request.clone();
            let name = (*name).to_owned();
            button(theme, format!("{id}-{suffix}"), glyph.into(), false).on_click(cx.listener(
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.change_widget(
                        &request,
                        &name,
                        Value::Number(snap(value + delta, min, max, step)),
                        cx,
                    );
                },
            ))
        };
        let slider = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(stepper("minus", "−", -step, cx))
                    .child(track)
                    .child(stepper("plus", "+", step, cx))
                    .child(div().min_w(px(44.)).text_right().child(value.to_string())),
            )
            .child(label(theme, &format!("{min} to {max}")));
        field(theme, control.label, slider)
    }

    /// Unhandled keys reach the native input handler; chat's focus guards
    /// keep them out of the composer and terminal.
    pub(super) fn widget_input(
        &self,
        card: &Card<'_>,
        control: Control<'_>,
        placeholder: &str,
        shown: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let input = card
            .form
            .and_then(|form| form.inputs.get(control.name))
            .map(|input| input.entity.clone());
        let request = card.request.clone();
        let name = control.name.to_owned();
        let placeholder = placeholder.to_owned();
        let id = control.id;
        let body = div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id)
            .min_h(px(36.))
            .px(px(9.))
            .py(px(6.))
            .min_w_0()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.background))
            .cursor_text()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let body = match input {
            Some(input) => {
                let font = self.chat_font();
                input.update(cx, |input, cx| {
                    input.set_appearance(font, self.theme.clone(), cx)
                });
                body.child(input)
            }
            None => body
                .text_color(rgb(if shown.is_empty() {
                    theme.muted
                } else {
                    theme.foreground
                }))
                .child(if shown.is_empty() {
                    placeholder.clone()
                } else {
                    shown
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.edit_widget_input(&request, &name, &placeholder, window, cx);
                })),
        };
        field(theme, control.label, body)
    }

    pub(super) fn widget_table(&self, columns: &[String], rows: &[Vec<String>]) -> Div {
        let theme = &self.theme;
        let row = |cells: &[String], heading: bool| {
            div()
                .flex()
                .min_w_0()
                .bg(rgb(if heading { theme.active } else { theme.surface }))
                .children(cells.iter().map(|cell| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .p(px(8.))
                        .when(heading, |cell| cell.font_weight(FontWeight::SEMIBOLD))
                        .child(cell.clone())
                }))
        };
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .rounded(px(corners::CONTROL))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(theme.active))
            .child(row(columns, true))
            .children(rows.iter().map(|cells| row(cells, false)))
    }

    pub(super) fn widget_chart(&self, title: &str, points: &[Point]) -> Div {
        let theme = &self.theme;
        let maximum = points
            .iter()
            .map(|point| point.value)
            .fold(0., f64::max)
            .max(1.);
        let bars = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(points.iter().map(|point| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(px(8.))
                            .child(point.label.clone())
                            .child(point.value.to_string()),
                    )
                    .child(
                        div()
                            .h(px(7.))
                            .w_full()
                            .rounded_full()
                            .bg(rgb(theme.active))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative((point.value / maximum) as f32))
                                    .rounded_full()
                                    .bg(rgb(theme.primary())),
                            ),
                    )
            }));
        field(theme, title, bars)
    }

    /// Puts the action's prompt in the composer, or sends it, on a click;
    /// a transcript that takes no prompts shows it disabled.
    pub(super) fn widget_action_button(
        &self,
        card: &Card<'_>,
        id: String,
        title: &str,
        prompt: &str,
        mode: ActionMode,
        cx: &mut Context<Self>,
    ) -> Div {
        let read_only = card.input != ChatInput::Composer;
        let request = card.request.clone();
        let prompt = prompt.to_owned();
        let caption = match mode {
            ActionMode::Draft => "Add to message",
            ActionMode::Send => "Send to agent",
        };
        let caption = if read_only {
            "Read-only transcript"
        } else {
            caption
        };
        div().flex().flex_col().gap(px(5.)).child(
            crate::accessibility::disabled(
                button(&self.theme, id, format!("{title} · {caption}"), !read_only),
                read_only,
            )
            .when(read_only, |button| button.opacity(0.5))
            .when(!read_only, |button| {
                button.on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.widget_action(&request, &prompt, mode, window, cx);
                }))
            }),
        )
    }
}

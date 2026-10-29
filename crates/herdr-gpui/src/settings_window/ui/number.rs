//! A number with typed entry, and -/+ steppers and an optional drag slider
//! unless it is a bare field.
use crate::config::{Theme, corners};
use crate::i18n::fl;
use crate::search_input::SearchInput;
use gpui::{prelude::*, *};
use std::{cell::Cell, ops::RangeInclusive, rc::Rc};

thread_local! {
    /// The last previewed value of the slider being dragged. Every Preview
    /// re-renders the page and rebuilds the slider's closures, so the value
    /// cannot live in them; GPUI runs one drag at a time, so one slot does.
    static PREVIEW: Cell<Option<(&'static str, f32)>> = const { Cell::new(None) };
}

fn remember_preview(id: &'static str, value: f32) {
    PREVIEW.with(|slot| slot.set(Some((id, value))));
}

/// The preview to commit on release; `None` when `id` is not being dragged,
/// so unrelated clicks commit nothing.
fn take_preview(id: &'static str) -> Option<f32> {
    PREVIEW.with(|slot| match slot.get() {
        Some((owner, value)) if owner == id => {
            slot.set(None);
            Some(value)
        }
        _ => None,
    })
}

pub(in crate::settings_window) struct Number {
    pub id: &'static str,
    pub value: f32,
    pub range: RangeInclusive<f32>,
    pub unit: Unit,
    pub look: Look,
    pub enabled: bool,
}

/// What the value counts, written after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::settings_window) enum Unit {
    /// A bare count, with nothing after it.
    None,
    Percent,
    Pixels,
    Seconds,
}

impl Unit {
    fn label(self) -> String {
        match self {
            Self::None => String::new(),
            Self::Percent => fl!("settings-unit-percent"),
            Self::Pixels => fl!("settings-unit-pixels"),
            Self::Seconds => fl!("settings-unit-seconds"),
        }
    }
}

/// What surrounds the typed value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::settings_window) enum Look {
    /// A drag slider, -/+ steppers, and the range beside them.
    Slider,
    /// -/+ steppers and the range.
    Steppers,
    /// Only the value, for numbers nobody steps through, such as a port.
    Field,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::settings_window) enum NumberChange {
    Preview(f32),
    Commit(f32),
    /// A release that ends no drag of ours: the preview is dropped unwritten.
    Abandon,
}

/// The typed-entry state the view keeps while one field is being edited.
pub(in crate::settings_window) struct NumberEdit {
    pub field: crate::settings_window::inputs::NumberField,
    pub input: Entity<SearchInput>,
    pub invalid: bool,
    pub _blur: Subscription,
}

pub(in crate::settings_window) fn parse_number(
    text: &str,
    range: &RangeInclusive<f32>,
) -> Option<f32> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = text.parse::<u16>().ok().map(f32::from)?;
    range.contains(&value).then_some(value)
}

pub(in crate::settings_window) fn slider_value(
    x: Pixels,
    bounds: Bounds<Pixels>,
    range: &RangeInclusive<f32>,
) -> f32 {
    let width = f32::from(bounds.size.width).max(1.);
    let fraction = ((f32::from(x) - f32::from(bounds.left())) / width).clamp(0., 1.);
    (range.start() + fraction * (range.end() - range.start())).round()
}

/// One step from `value`, rounded first so a fractional config value still
/// lands on a whole number; None for a non-finite value or step.
fn stepped_value(value: f32, delta: f32, range: &RangeInclusive<f32>) -> Option<f32> {
    if !value.is_finite() || !delta.is_finite() {
        return None;
    }
    Some((value.round() + delta.round()).clamp(*range.start(), *range.end()))
}

#[derive(Clone, Copy, PartialEq)]
struct SliderDrag(&'static str);

pub(in crate::settings_window) fn number(
    theme: &Theme,
    spec: Number,
    editing: Option<&NumberEdit>,
    on_change: impl Fn(NumberChange, &mut Window, &mut App) + 'static,
    on_edit: impl Fn(&mut Window, &mut App) + 'static,
) -> Div {
    let on_change = Rc::new(on_change);
    let id = spec.id;
    let enabled = spec.enabled;
    let range = spec.range.clone();
    let mut root = div()
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(10.))
        .flex_none()
        .when(!enabled, |root| root.opacity(0.5));
    if spec.look == Look::Slider {
        let fraction =
            ((spec.value - range.start()) / (range.end() - range.start()).max(1.)).clamp(0., 1.);
        let commit = {
            let on_change = on_change.clone();
            move |_: &MouseUpEvent, window: &mut Window, cx: &mut App| {
                // A release with no drag running cannot be ours: a drag that
                // ended without reaching us (unmounted slider) left a stale
                // value behind.
                match (take_preview(id), cx.has_active_drag()) {
                    (Some(value), true) => on_change(NumberChange::Commit(value), window, cx),
                    (Some(_), false) => on_change(NumberChange::Abandon, window, cx),
                    (None, _) => {}
                }
            }
        };
        let track = div()
            .id((ElementId::from(id), "slider"))
            .debug_selector(move || format!("{id}-slider"))
            .w(px(150.))
            .h(px(16.))
            .flex()
            .items_center()
            .child(
                div()
                    .w_full()
                    .h(px(3.))
                    .rounded_full()
                    .bg(rgb(theme.active))
                    .child(
                        div()
                            .h_full()
                            .w(relative(fraction))
                            .rounded_full()
                            .bg(rgb(theme.primary())),
                    ),
            );
        root = root.child(if enabled {
            let on_change = on_change.clone();
            let range = range.clone();
            track
                .cursor_pointer()
                .on_drag(SliderDrag(id), move |_, _, _, cx| {
                    // A new drag starts from nothing, never a leftover value.
                    take_preview(id);
                    cx.new(|_| Empty)
                })
                .on_drag_move(move |event: &DragMoveEvent<SliderDrag>, window, cx| {
                    if event.drag(cx).0 == id {
                        let value = slider_value(event.event.position.x, event.bounds, &range);
                        remember_preview(id, value);
                        on_change(NumberChange::Preview(value), window, cx);
                    }
                })
                .on_mouse_up(MouseButton::Left, commit.clone())
                .on_mouse_up_out(MouseButton::Left, commit)
                .into_any_element()
        } else {
            track.into_any_element()
        });
    }
    let value_text = format!("{}", spec.value as i32);
    let steppers = spec.look != Look::Field;
    let width = px(if steppers { 44. } else { 56. });
    let border = match editing {
        Some(edit) if edit.invalid => theme.ink(0xd45b5b),
        _ => theme.active,
    };
    let shown = match editing {
        Some(edit) => div()
            .w(width)
            .px(px(6.))
            .child(edit.input.clone())
            .into_any_element(),
        None => div()
            .id((ElementId::from(id), "value"))
            .w(width)
            .px(px(6.))
            .text_right()
            .when(enabled, |value| value.cursor_text())
            .child(value_text)
            .when(enabled, |value| {
                value.on_click(move |_, window, cx| on_edit(window, cx))
            })
            .into_any_element(),
    };
    let stepper_for = |which: &'static str, glyph: &'static str, delta: f32| {
        let on_change = on_change.clone();
        let range = range.clone();
        let value = spec.value;
        stepper(theme, id, which, glyph, enabled, move |window, cx| {
            if let Some(next) = stepped_value(value, delta, &range) {
                on_change(NumberChange::Commit(next), window, cx)
            }
        })
    };
    let field = div()
        .debug_selector(move || format!("{id}-field"))
        .flex()
        .items_center()
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(theme.surface))
        .child(shown)
        .child(
            div()
                .pr(px(6.))
                .text_color(rgb(theme.muted))
                .child(spec.unit.label()),
        )
        .when(steppers, |field| {
            field
                .child(stepper_for("minus", "\u{2212}", -1.))
                .child(stepper_for("plus", "+", 1.))
        });
    // The native desktop checks click the field by its painted bounds.
    #[cfg(all(feature = "integration-test", target_os = "macos"))]
    let field = field
        .relative()
        .child(crate::settings_window::controls::native_probe(format!(
            "{id}-field"
        )));
    root.child(field).when(steppers, |root| {
        root.child(
            div()
                .text_size(px(10.))
                .text_color(rgb(theme.muted))
                .child(fl!(
                    "settings-number-range",
                    min = *range.start() as i32,
                    max = *range.end() as i32
                )),
        )
    })
}

fn stepper(
    theme: &Theme,
    id: &'static str,
    which: &'static str,
    glyph: &'static str,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id((ElementId::from(id), which))
        .debug_selector(move || format!("{id}-{which}"))
        .w(px(20.))
        .py(px(3.))
        .text_center()
        .border_l_1()
        .border_color(rgb(theme.active))
        .text_color(rgb(theme.subtext()))
        .when(enabled, |stepper| {
            stepper
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme.active)))
                .on_click(move |_, window, cx| on_click(window, cx))
        })
        .child(glyph)
}

#[cfg(test)]
mod tests;

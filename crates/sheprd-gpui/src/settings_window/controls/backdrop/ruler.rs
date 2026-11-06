//! The backdrop's opacity ruler: a 10–100% track with a tick every 5% and
//! labeled marks. A click lands anywhere at once and a drag follows the
//! pointer; either is written once, when the button is released, so a drag
//! is one save rather than one per pixel.
use super::*;
use std::{cell::Cell, rc::Rc};

/// The ruler's ends, in percent.
const MIN_PERCENT: i32 = 10;
const MAX_PERCENT: i32 = 100;
/// Ticks this many percent apart; the labeled marks also jump there.
const TICK: usize = 5;
const MARKS: [i32; 5] = [10, 25, 50, 75, 100];
/// The knob's diameter and the track's height around it.
const KNOB: f32 = 16.;
const TRACK: f32 = 22.;

/// What the ruler's track drags.
#[derive(Clone, Copy, Debug)]
struct Knob;

/// The ruler's state between renders.
#[derive(Default)]
pub(in crate::settings_window) struct Ruler {
    /// The track's bounds as last painted, for a click to land on.
    track: Rc<Cell<Bounds<Pixels>>>,
    /// The opacity under the pointer while the button is down, not yet saved.
    held: Option<f32>,
    /// The opacity being written, shown until the write finishes. It is kept
    /// apart from the config, so a failed write leaves the saved value shown
    /// and a click on the same value tries again.
    saving: Option<f32>,
}

impl Ruler {
    /// Forgets the value being written once no write runs, whether it
    /// succeeded or not; the reloaded config shows what was saved.
    pub(in crate::settings_window) fn settle(&mut self) {
        self.saving = None;
    }
}

/// Where `opacity` sits along the ruler, from 0 to 1.
pub(in crate::settings_window) fn fraction(opacity: f32) -> f32 {
    let percent = opacity * 100.;
    ((percent - MIN_PERCENT as f32) / (MAX_PERCENT - MIN_PERCENT) as f32).clamp(0., 1.)
}

/// The opacity at `fraction` along the ruler, on a whole percent.
pub(in crate::settings_window) fn opacity_at(fraction: f32) -> f32 {
    let span = (MAX_PERCENT - MIN_PERCENT) as f32;
    let percent = MIN_PERCENT + (fraction.clamp(0., 1.) * span).round() as i32;
    percent as f32 / 100.
}

impl SettingsWindow {
    fn hold_opacity(&mut self, x: Pixels, track: Bounds<Pixels>, cx: &mut Context<Self>) {
        let width = f32::from(track.size.width).max(1.);
        let held = opacity_at(f32::from(x - track.left()) / width);
        if self.controls.ruler.held != Some(held) {
            self.controls.ruler.held = Some(held);
            cx.notify();
        }
    }

    fn release_opacity(&mut self, cx: &mut Context<Self>) {
        let Some(held) = self.controls.ruler.held.take() else {
            return;
        };
        cx.notify();
        self.set_opacity(held, cx);
    }

    fn set_opacity(&mut self, opacity: f32, cx: &mut Context<Self>) {
        if self.busy() || opacity == self.config.backdrop.opacity {
            return;
        }
        self.save_backdrop(vec![Edit::Opacity(opacity)], cx);
        // Shown at once rather than after the reload, which confirms it.
        self.controls.ruler.saving = self.saving.then_some(opacity);
    }

    pub(in crate::settings_window) fn backdrop_ruler(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let accent = crate::menu::accent(theme);
        let on_accent = [accent.r, accent.g, accent.b]
            .into_iter()
            .fold(0, |color, channel| {
                (color << 8) | (channel * 255.).round() as u32
            });
        let ruler = &self.controls.ruler;
        let opacity = ruler
            .held
            .or(ruler.saving.filter(|_| self.saving))
            .unwrap_or(self.config.backdrop.opacity);
        let along = fraction(opacity);
        let percent = (opacity * 100.).round() as i32;
        let at = |percent: i32| relative(fraction(percent as f32 / 100.));
        let ticks = (MIN_PERCENT..=MAX_PERCENT).step_by(TICK).map(|tick| {
            let major = MARKS.contains(&tick);
            div()
                .absolute()
                .top_0()
                .left(at(tick))
                .w(px(1.))
                .h(px(if major { 7. } else { 4. }))
                .bg(rgb(if major { theme.muted } else { theme.active }))
        });
        let marks = MARKS.map(|mark| {
            let color = if mark == percent {
                theme.foreground
            } else {
                theme.muted
            };
            let hover = theme.foreground;
            div()
                .id(("settings-backdrop-mark", mark as usize))
                .debug_selector(move || format!("settings-backdrop-mark-{mark}"))
                .absolute()
                .top_0()
                .left(at(mark))
                .ml(px(-18.))
                .w(px(36.))
                .flex()
                .justify_center()
                .cursor_pointer()
                .text_size(px(11.))
                .text_color(rgb(color))
                .hover(move |style| style.text_color(rgb(hover)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_opacity(mark as f32 / 100., cx);
                }))
                .child(mark.to_string())
        });
        let bubble = div()
            .absolute()
            .top_0()
            .left(relative(along))
            .ml(px(-22.))
            .w(px(44.))
            .flex()
            .justify_center()
            .child(
                div()
                    .debug_selector(|| "settings-backdrop-opacity".into())
                    .px(px(6.))
                    .rounded(px(corners::SMALL))
                    .bg(accent)
                    .text_color(rgb(theme.text_on(on_accent)))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("settings-theme-backdrop-percent", percent = percent)),
            );
        let recorded = self.controls.ruler.track.clone();
        let track = div()
            .id("settings-backdrop-ruler")
            .debug_selector(|| "settings-backdrop-ruler".into())
            .relative()
            .h(px(TRACK))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(
                div()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(rgb(theme.active))
                    .child(div().h_full().w(relative(along)).rounded_full().bg(accent)),
            )
            .child(
                div()
                    .debug_selector(|| "settings-backdrop-knob".into())
                    .absolute()
                    .top(px((TRACK - KNOB) / 2.))
                    .left(relative(along))
                    .ml(px(-KNOB / 2.))
                    .size(px(KNOB))
                    .rounded_full()
                    .bg(rgb(theme.foreground))
                    .border_2()
                    .border_color(accent),
            )
            .child(
                canvas(move |bounds, _, _| recorded.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    let track = this.controls.ruler.track.get();
                    this.hold_opacity(event.position.x, track, cx);
                }),
            )
            .on_drag(Knob, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<Knob>, _, cx| {
                this.hold_opacity(event.event.position.x, event.bounds, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.release_opacity(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.release_opacity(cx)),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .py(px(8.))
            .child(fl!("settings-theme-backdrop-opacity"))
            .child(
                div()
                    // Room for the knob and the end labels to overhang.
                    .px(px(KNOB / 2.))
                    .flex()
                    .flex_col()
                    .child(div().relative().h(px(20.)).child(bubble))
                    .child(track)
                    .child(div().relative().h(px(8.)).children(ticks))
                    .child(div().relative().h(px(14.)).children(marks)),
            )
    }
}

#[cfg(test)]
impl Controls {
    pub(in crate::settings_window) fn ruler_held(&self) -> Option<f32> {
        self.ruler.held
    }

    pub(in crate::settings_window) fn set_ruler_saving(&mut self, saving: Option<f32>) {
        self.ruler.saving = saving;
    }
}

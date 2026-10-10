//! Back and Forward beside the sidebar toggle, drawn the way each platform's
//! own toolbars draw them. Only the look differs; every platform walks the
//! same focus trail (`endpoint::history`).

use crate::{HerdrWindow, config::corners, endpoint::Step};
use gpui::{prelude::*, *};

/// A platform's toolbar convention for the pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    /// AppKit's segmented control, as Finder and Xcode join the two: one
    /// rounded bezel split by a hairline, chevrons inside.
    Segmented,
    /// Windows 11's Fluent subtle buttons, as File Explorer sets them:
    /// arrows on transparent squares that fill only under the pointer.
    Fluent,
    /// GNOME's flat header bar buttons, as Files sets them: chevrons on
    /// rounded transparent buttons a little apart.
    Adwaita,
}

impl Style {
    pub(crate) const NATIVE: Self = if cfg!(target_os = "macos") {
        Self::Segmented
    } else if cfg!(windows) {
        Self::Fluent
    } else {
        Self::Adwaita
    };

    /// Each button's size and corner radius.
    pub(crate) fn button(self) -> (Size<Pixels>, f32) {
        match self {
            Self::Segmented => (size(px(30.), px(22.)), corners::SMALL + 2.),
            Self::Fluent => (size(px(32.), px(28.)), corners::SMALL),
            Self::Adwaita => (size(px(30.), px(28.)), corners::SMALL + 2.),
        }
    }

    /// The pair's whole width, margins and divider included.
    pub(crate) fn width(self) -> f32 {
        let button = f32::from(self.button().0.width);
        match self {
            // 2px lead, the bezel's 1px border on each side, and the hairline.
            Self::Segmented => 2. + 2. + 1. + 2. * button,
            Self::Fluent => 4. + 2. * button,
            Self::Adwaita => 6. + 2. * button,
        }
    }

    fn icon(self, step: Step) -> &'static str {
        match (self, step) {
            (Self::Fluent, Step::Back) => "icons/arrow-left.svg",
            (Self::Fluent, Step::Forward) => "icons/arrow-right.svg",
            // The chevron points right; Back turns it around.
            _ => "icons/chevron-right.svg",
        }
    }
}

/// `color` at `alpha`, so a fill reads the same over the header, a tab
/// strip, or the sidebar's color behind it.
fn wash(color: u32, alpha: u8) -> Rgba {
    rgba((color << 8) | u32::from(alpha))
}

impl HerdrWindow {
    pub(super) fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let style = Style::NATIVE;
        let theme = &self.theme;
        let (ink, surface) = (theme.foreground, theme.surface);
        let button = |step: Step| {
            let enabled = self.can_travel(step);
            let (frame, radius) = style.button();
            let (id, label) = match step {
                Step::Back => ("titlebar-back", "Back"),
                Step::Forward => ("titlebar-forward", "Forward"),
            };
            let (hover, pressed) = match style {
                Style::Segmented => (0x14, 0x24),
                Style::Fluent => (0x0f, 0x0a),
                Style::Adwaita => (0x12, 0x29),
            };
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .w(frame.width)
                .h(frame.height)
                .when(style != Style::Segmented, |button| {
                    button.rounded(px(radius))
                })
                .child(
                    svg()
                        .path(style.icon(step))
                        .size(px(if style == Style::Segmented { 14. } else { 16. }))
                        .flex_none()
                        .text_color(rgb(if enabled { ink } else { theme.muted }))
                        .when(!enabled, |icon| icon.opacity(0.5))
                        .when(step == Step::Back && style != Style::Fluent, |icon| {
                            icon.with_transformation(Transformation::rotate(radians(
                                std::f32::consts::PI,
                            )))
                        }),
                )
                // A press never moves the window, even on a disabled button.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(wash(ink, hover)))
                        .active(move |button| button.bg(wash(ink, pressed)))
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::usage::Hint {
                                text: label.into(),
                                foreground: ink,
                                surface,
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.travel(step, cx);
                            window.focus(&this.focus, cx);
                        }))
                })
        };
        let pair = div()
            .debug_selector(|| "titlebar-navigation".into())
            .flex()
            .flex_none()
            .items_center()
            .self_center();
        match style {
            Style::Segmented => pair
                .ml(px(2.))
                .rounded(px(style.button().1))
                .overflow_hidden()
                .bg(wash(ink, 0x0f))
                .border_1()
                .border_color(wash(ink, 0x1a))
                .child(button(Step::Back))
                .child(div().w(px(1.)).h(px(14.)).bg(wash(ink, 0x24)))
                .child(button(Step::Forward)),
            Style::Fluent => pair
                .gap(px(4.))
                .child(button(Step::Back))
                .child(button(Step::Forward)),
            Style::Adwaita => pair
                .gap(px(6.))
                .child(button(Step::Back))
                .child(button(Step::Forward)),
        }
    }
}

#[cfg(test)]
mod tests;

//! Rounded pills floating on the strip, without rules between them. Each
//! leads with a chip for what runs in it, the agent's logo or a terminal,
//! and ends with its status, which gives way to the close button under the
//! pointer.

use super::{
    DOT_GAP, FLOATING_PADDING, TabContext, TabFace, TabLayout, TabShape, TabState, ZOOM_ICON,
    close_button, floating_new_tab, kind_icon, status_mark, zoom_mark,
};
use crate::{
    config::{FontConfig, Theme, corners, mix},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

/// Space around each pill, inside the tab, so pills never touch.
const MARGIN_X: f32 = 2.;
const MARGIN_Y: f32 = 4.;

/// The padding inside a pill, before its chip and after its end slot.
const PAD_LEFT: f32 = 5.;
const PAD_RIGHT: f32 = 4.;

/// The rounded square leading a pill, and the mark inside it.
const CHIP: f32 = 18.;
const CHIP_ICON: f32 = 11.;

/// The gap between a pill's parts.
const GAP: f32 = 8.;

/// The slot after the title that holds the status, then the close button.
const END: f32 = 16.;

pub(super) struct Pills;

impl TabLayout for Pills {
    fn width(&self, title: f32, shape: TabShape, _: &FontConfig, _: Indicators) -> f32 {
        // Every pill has a chip and an end slot, whatever leads it.
        let zoom = if shape.zoomed {
            DOT_GAP + ZOOM_ICON
        } else {
            0.
        };
        (2. * MARGIN_X + PAD_LEFT + CHIP + GAP + title + zoom + GAP + END + PAD_RIGHT)
            .max(crate::TAB_WIDTH)
    }

    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32) {
        match (state.selected, state.active) {
            (true, true) => (theme.active, theme.foreground),
            (true, false) => (mix(theme.surface, theme.active, 50), theme.foreground),
            (false, _) => (theme.surface, theme.muted),
        }
    }

    fn tab(
        &self,
        base: Stateful<Div>,
        face: TabFace,
        state: TabState,
        cx: &TabContext<'_>,
    ) -> Stateful<Div> {
        let TabFace {
            label,
            mark,
            zoomed,
            probe,
            close,
        } = face;
        let (background, text) = self.colors(state, cx.theme);
        let hover = probe.hover_group();
        let wash = mix(cx.theme.surface, cx.theme.active, 45);
        let chip_fill = mix(background, cx.theme.foreground, 12);
        let chip = div()
            .flex_none()
            .size(px(CHIP))
            .rounded(px(corners::SMALL))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(chip_fill))
            .child(kind_icon(&mark, &probe, CHIP_ICON, text, cx));
        // The status shows until the pointer is over the pill, when the close
        // button takes its place. While the strip takes Tab focus, the close
        // button is a stop and shows throughout.
        let keyboard = cx.keyboard();
        let status = status_mark(&mark, &probe, cx).filter(|_| !keyboard);
        let end = div()
            .relative()
            .flex_none()
            .size(px(END))
            .flex()
            .items_center()
            .justify_center()
            .children(status.map(|status| {
                div()
                    .group_hover(hover.clone(), |s| s.opacity(0.))
                    .child(status)
            }))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .when(!keyboard, |slot| {
                        slot.opacity(0.)
                            .group_hover(hover.clone(), |s| s.opacity(1.))
                    })
                    .child(close_button(close, &probe, END, text, cx)),
            );
        base.group(hover)
            .px(px(MARGIN_X))
            .py(px(MARGIN_Y))
            .flex_none()
            .flex()
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .min_w(px(crate::TAB_WIDTH - 2. * MARGIN_X))
                    .h_full()
                    .pl(px(PAD_LEFT))
                    .pr(px(PAD_RIGHT))
                    .rounded(px(corners::CONTROL))
                    .bg(cx.fill.over(cx.theme.surface, background))
                    .when(!state.selected, |pill| pill.hover(move |s| s.bg(rgb(wash))))
                    .text_color(rgb(text))
                    .child(chip)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(DOT_GAP))
                            .child(label)
                            .children(zoom_mark(zoomed, &probe, text, cx)),
                    )
                    .child(end),
            )
    }

    fn padding(&self) -> f32 {
        FLOATING_PADDING
    }

    fn new_tab(&self, base: Stateful<Div>, icon: Svg, theme: &Theme) -> Stateful<Div> {
        floating_new_tab(base, icon, theme)
    }
}

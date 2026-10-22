//! Browser-like tabs: the chosen one is a raised card, open at its foot onto
//! what the group shows, and the rest rest on the strip, washed under the
//! pointer. Each leads with what runs in it and ends with its status.

use super::{
    DOT_GAP, FLOATING_PADDING, TabContext, TabFace, TabLayout, TabShape, TabState, ZOOM_ICON,
    close_button, floating_new_tab, kind_icon, status_mark, status_width, zoom_mark,
};
use crate::{
    config::{FontConfig, Theme, corners, mix},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

/// How far the cards stand below the strip's top edge.
const TOP: f32 = 5.;

const PAD_LEFT: f32 = 10.;
const PAD_RIGHT: f32 = 6.;

/// The gap between a card's parts.
const GAP: f32 = 7.;

const ICON: f32 = 12.;
const CLOSE: f32 = 16.;

/// The card's border, on both sides of every card so they keep one width.
const BORDER: f32 = 1.;

pub(super) struct Cards;

impl TabLayout for Cards {
    fn width(&self, title: f32, shape: TabShape, font: &FontConfig, indicators: Indicators) -> f32 {
        let status = status_width(shape.lead, font, indicators).map_or(0., |dot| dot + GAP);
        let zoom = if shape.zoomed {
            DOT_GAP + ZOOM_ICON
        } else {
            0.
        };
        (2. * BORDER + PAD_LEFT + ICON + GAP + title + zoom + GAP + status + CLOSE + PAD_RIGHT)
            .max(crate::TAB_WIDTH)
    }

    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32) {
        match (state.selected, state.active) {
            (true, true) => (theme.background, theme.foreground),
            (true, false) => (theme.background, theme.subtext()),
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
        let (_, text) = self.colors(state, cx.theme);
        let theme = cx.theme;
        let hover = probe.hover_group();
        let wash = mix(theme.surface, theme.active, 45);
        let card = div()
            .h_full()
            .flex()
            .items_center()
            .gap(px(GAP))
            .pl(px(PAD_LEFT))
            .pr(px(PAD_RIGHT))
            .rounded_t(px(corners::PANEL))
            .border_1()
            .border_b_0()
            .text_color(rgb(text))
            .map(|card| {
                if state.selected {
                    card.bg(cx.fill.ground()).border_color(rgb(theme.active))
                } else {
                    card.border_color(transparent_black())
                        .hover(move |s| s.bg(rgb(wash)))
                }
            })
            .child(kind_icon(&mark, &probe, ICON, text, cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(DOT_GAP))
                    .child(label)
                    .children(zoom_mark(zoomed, &probe, text, cx)),
            )
            .children(status_mark(&mark, &probe, cx))
            .child(
                div()
                    .flex_none()
                    .when(!state.selected && !cx.keyboard(), |slot| {
                        slot.opacity(0.)
                            .group_hover(hover.clone(), |s| s.opacity(1.))
                    })
                    .child(close_button(close, &probe, CLOSE, text, cx)),
            );
        base.group(hover)
            .min_w(px(crate::TAB_WIDTH))
            .h_full()
            .pt(px(TOP))
            .flex_none()
            .flex()
            .flex_col()
            .cursor_pointer()
            .child(card.flex_1().min_h_0())
    }

    fn padding(&self) -> f32 {
        FLOATING_PADDING
    }

    fn new_tab(&self, base: Stateful<Div>, icon: Svg, theme: &Theme) -> Stateful<Div> {
        floating_new_tab(base, icon, theme)
    }
}

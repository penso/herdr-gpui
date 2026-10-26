//! Tabs joined to what they show, as 1git draws them: the chosen tab is a
//! body in the content's own color whose foot curves out into the card
//! below, painted by the strip so it can slide between tabs. The rest are
//! bare titles, washed in a rounded pill under the pointer.

use super::{
    Connected, DOT_GAP, FLOATING_NEW_TAB_WIDTH, TabContext, TabFace, TabLayout, TabShape, TabState,
    ZOOM_ICON, close_button, floating_new_tab, kind_icon, status_mark, status_width, zoom_mark,
};
use crate::{
    config::{FontConfig, Theme, mix},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

/// How far the body stands below the strip's top edge.
const TOP: f32 = 5.;

/// The body's top corners and the flare of its shoulders.
const RADIUS: f32 = 8.;
const SHOULDER: f32 = 10.;

/// The chrome around the content card, and the card's corners.
const INSET: f32 = 6.;
const CARD_RADIUS: f32 = 10.;

const PAD_LEFT: f32 = 12.;
const PAD_RIGHT: f32 = 6.;

/// The gap between a tab's parts.
const GAP: f32 = 7.;

const ICON: f32 = 12.;
const CLOSE: f32 = 16.;

/// How far the hover pill keeps inside the tab, sideways and at its foot,
/// so neighbouring pills never touch and the pill clears the shoulders.
const PILL_SIDE: f32 = 2.;
const PILL_FOOT: f32 = 3.;

pub(super) struct Curved;

impl TabLayout for Curved {
    fn width(&self, title: f32, shape: TabShape, font: &FontConfig, indicators: Indicators) -> f32 {
        let status = status_width(shape.lead, font, indicators).map_or(0., |dot| dot + GAP);
        let zoom = if shape.zoomed {
            DOT_GAP + ZOOM_ICON
        } else {
            0.
        };
        (PAD_LEFT + ICON + GAP + title + zoom + GAP + status + CLOSE + PAD_RIGHT)
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
        let pill = div()
            .absolute()
            .top_0()
            .bottom(px(PILL_FOOT))
            .left(px(PILL_SIDE))
            .right(px(PILL_SIDE))
            .rounded(px(RADIUS - PILL_SIDE))
            .when(!state.selected, |pill| {
                pill.group_hover(hover.clone(), move |s| s.bg(rgb(wash)))
            });
        let content = div()
            .relative()
            .h_full()
            .flex()
            .items_center()
            .gap(px(GAP))
            .pl(px(PAD_LEFT))
            .pr(px(PAD_RIGHT))
            .pb(px(PILL_FOOT))
            .text_color(rgb(text))
            .when(state.selected, |content| {
                content.font_weight(FontWeight::MEDIUM)
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
            .relative()
            .min_w(px(crate::TAB_WIDTH))
            .h_full()
            .pt(px(TOP))
            .flex_none()
            .flex()
            .flex_col()
            .cursor_pointer()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(pill)
                    .child(content),
            )
    }

    /// Room for the shoulders and the card's chrome, so the first and last
    /// bodies flare into the card rather than past it.
    fn padding(&self) -> f32 {
        INSET + SHOULDER
    }

    fn new_tab(&self, base: Stateful<Div>, icon: AnyElement, theme: &Theme) -> Stateful<Div> {
        floating_new_tab(base, icon, theme)
    }

    fn new_tab_width(&self) -> f32 {
        FLOATING_NEW_TAB_WIDTH
    }

    fn connected(&self) -> Option<Connected> {
        Some(Connected {
            top: TOP,
            radius: RADIUS,
            shoulder: SHOULDER,
            inset: INSET,
            card_radius: CARD_RADIUS,
        })
    }
}

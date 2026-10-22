//! Herdr UI's first tabs: even cells divided by a single rule, the chosen
//! one washed in the accent while its group has the keyboard.

use super::{
    Lead, TabContext, TabFace, TabLayout, TabShape, TabState, close_button, lead_mark, zoom_mark,
};
use crate::{
    config::{FontConfig, Theme},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

/// The gap between a tab's mark and its title.
pub(crate) const DOT_GAP: f32 = 6.;

/// How large the mark after a zoomed tab's title draws, `DOT_GAP` after it.
pub(crate) const ZOOM_ICON: f32 = 11.;

/// A browser tab's icon.
const ICON: f32 = 12.;

const CLOSE: f32 = 18.;

/// The close button hugs the tab's inner right edge, well clear of the
/// label it would otherwise crowd.
const PAD_RIGHT: f32 = 3.;

/// The rule after each tab.
const RULE: f32 = 1.;

pub(super) struct Classic;

impl Classic {
    /// The padding before a tab's content and the gap before its close
    /// button. A browser tab's icon sits closer to both.
    const fn spacing(lead: Lead) -> (f32, f32) {
        match lead {
            Lead::Icon => (10., 6.),
            Lead::Nothing | Lead::Status => (12., 10.),
        }
    }
}

impl TabLayout for Classic {
    fn width(
        &self,
        title: f32,
        TabShape { lead, zoomed, .. }: TabShape,
        font: &FontConfig,
        indicators: Indicators,
    ) -> f32 {
        let (pad, gap) = Self::spacing(lead);
        let mark = match lead {
            Lead::Nothing => 0.,
            Lead::Status => indicators.width(font) + DOT_GAP,
            Lead::Icon => ICON + DOT_GAP,
        };
        let zoom = if zoomed { DOT_GAP + ZOOM_ICON } else { 0. };
        (pad + mark + title + zoom + gap + CLOSE + PAD_RIGHT + RULE).max(crate::TAB_WIDTH)
    }

    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32) {
        match (state.selected, state.active) {
            (true, true) => {
                let background = theme.primary_wash();
                (background, theme.text_on(background))
            }
            (true, false) => (theme.active, theme.foreground),
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
        let (pad, gap) = Self::spacing(mark.lead());
        base.pl(px(pad))
            .pr(px(PAD_RIGHT))
            .py(px(2.))
            .min_w(px(crate::TAB_WIDTH))
            .border_r_1()
            .border_color(rgb(cx.theme.active))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(gap))
            .cursor_pointer()
            .bg(cx.fill.over(cx.theme.surface, background))
            .text_color(rgb(text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(DOT_GAP))
                    .children(lead_mark(&mark, &probe, ICON, text, cx))
                    .child(label)
                    .children(zoom_mark(zoomed, &probe, text, cx)),
            )
            .child(close_button(close, &probe, CLOSE, text, cx))
    }
}

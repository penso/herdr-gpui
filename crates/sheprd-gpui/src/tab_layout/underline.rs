//! Flat titles without fills or rules. The chosen tab carries a bar along
//! its foot, in the accent while its group has the keyboard; the close
//! button shows on the chosen tab and under the pointer.

use super::{
    DOT_GAP, FLOATING_NEW_TAB_WIDTH, FLOATING_PADDING, Lead, TabContext, TabFace, TabLayout,
    TabShape, TabState, close_button, floating_new_tab, lead_mark, mute_mark, status_width,
    zoom_mark,
};
use crate::{
    config::{FontConfig, Theme},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

const PAD_LEFT: f32 = 10.;
const PAD_RIGHT: f32 = 6.;

/// The gap between a tab's mark, title, and close button.
const GAP: f32 = 7.;

/// A browser tab's icon.
const ICON: f32 = 12.;

const CLOSE: f32 = 16.;

/// The bar along the chosen tab's foot, inset from its sides.
const BAR: f32 = 2.;
const BAR_INSET: f32 = 6.;

pub(super) struct Underline;

impl TabLayout for Underline {
    fn panes(&self) -> &'static dyn super::panes::PaneChrome {
        &super::panes::Underline
    }

    fn width(
        &self,
        title: f32,
        shape @ TabShape { lead, .. }: TabShape,
        font: &FontConfig,
        indicators: Indicators,
    ) -> f32 {
        let mark = match lead {
            Lead::Nothing => 0.,
            Lead::Status => status_width(lead, font, indicators).unwrap_or(0.) + GAP,
            Lead::Icon => ICON + GAP,
        };
        let zoom = shape.marks_width();
        (PAD_LEFT + mark + title + zoom + GAP + CLOSE + PAD_RIGHT).max(crate::TAB_WIDTH)
    }

    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32) {
        if state.selected {
            (theme.surface, theme.foreground)
        } else {
            (theme.surface, theme.muted)
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
            muted,
            probe,
            close,
        } = face;
        let (_, text) = self.colors(state, cx.theme);
        let theme = cx.theme;
        let hover = probe.hover_group();
        let (foreground, resting) = (theme.foreground, theme.active);
        let bar = match (state.selected, state.active) {
            (true, true) => Some(theme.primary()),
            (true, false) => Some(theme.muted),
            (false, _) => None,
        };
        base.group(hover.clone())
            .min_w(px(crate::TAB_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .cursor_pointer()
            .text_color(rgb(text))
            .hover(move |s| s.text_color(rgb(foreground)))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .pl(px(PAD_LEFT))
                    .pr(px(PAD_RIGHT))
                    .children(lead_mark(&mark, &probe, ICON, text, cx))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(DOT_GAP))
                            .child(label)
                            .children(zoom_mark(zoomed, &probe, text, cx))
                            .children(mute_mark(muted, &probe, text, cx)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .when(!state.selected && !cx.keyboard(), |slot| {
                                slot.opacity(0.)
                                    .group_hover(hover.clone(), |s| s.opacity(1.))
                            })
                            .child(close_button(close, &probe, CLOSE, text, cx)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(BAR))
                    .mx(px(BAR_INSET))
                    .rounded_full()
                    .map(|foot| match bar {
                        Some(color) => foot.bg(rgb(color)),
                        None => foot.group_hover(hover, move |s| s.bg(rgb(resting))),
                    }),
            )
    }

    fn padding(&self) -> f32 {
        FLOATING_PADDING
    }

    fn new_tab(&self, base: Stateful<Div>, icon: AnyElement, theme: &Theme) -> Stateful<Div> {
        floating_new_tab(base, icon, theme)
    }

    fn new_tab_width(&self) -> f32 {
        FLOATING_NEW_TAB_WIDTH
    }
}

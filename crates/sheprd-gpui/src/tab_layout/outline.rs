//! Bare titles on the strip, the chosen one standing in an outlined pill
//! with its close button. Each leads with what runs in it and ends with its
//! status; the rest show their close button under the pointer.

use super::{
    DOT_GAP, FLOATING_NEW_TAB_WIDTH, FLOATING_PADDING, TabContext, TabFace, TabLayout, TabShape,
    TabState, close_button, floating_new_tab, kind_icon, mute_mark, status_mark, status_width,
    zoom_mark,
};
use crate::{
    config::{FontConfig, Theme, corners, mix},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

/// Space around each pill, inside the tab, so pills never touch.
const MARGIN_X: f32 = 2.;
const MARGIN_Y: f32 = 4.;

const PAD_LEFT: f32 = 10.;
const PAD_RIGHT: f32 = 5.;

/// The gap between a pill's parts.
const GAP: f32 = 7.;

const ICON: f32 = 12.;
const CLOSE: f32 = 16.;

/// The outline, on every pill so they keep one width.
const BORDER: f32 = 1.;

pub(super) struct Outline;

impl TabLayout for Outline {
    fn panes(&self) -> &'static dyn super::panes::PaneChrome {
        &super::panes::Classic
    }

    fn width(&self, title: f32, shape: TabShape, font: &FontConfig, indicators: Indicators) -> f32 {
        let status = status_width(shape.lead, font, indicators).map_or(0., |dot| dot + GAP);
        let zoom = shape.marks_width();
        (2. * (MARGIN_X + BORDER)
            + PAD_LEFT
            + ICON
            + GAP
            + title
            + zoom
            + GAP
            + status
            + CLOSE
            + PAD_RIGHT)
            .max(crate::TAB_WIDTH)
    }

    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32) {
        match (state.selected, state.active) {
            (true, true) => (mix(theme.surface, theme.active, 60), theme.foreground),
            (true, false) => (mix(theme.surface, theme.active, 30), theme.subtext()),
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
            muted,
            probe,
            close,
        } = face;
        let (background, text) = self.colors(state, cx.theme);
        let theme = cx.theme;
        let hover = probe.hover_group();
        let wash = mix(theme.surface, theme.active, 45);
        let outline = if state.active {
            theme.active
        } else {
            mix(theme.surface, theme.active, 70)
        };
        let pill = div()
            .flex()
            .items_center()
            .gap(px(GAP))
            .min_w(px(crate::TAB_WIDTH - 2. * MARGIN_X))
            .h_full()
            .pl(px(PAD_LEFT))
            .pr(px(PAD_RIGHT))
            .rounded(px(corners::CONTROL))
            .border_1()
            .text_color(rgb(text))
            .map(|pill| {
                if state.selected {
                    pill.bg(cx.fill.over(theme.surface, background))
                        .border_color(rgb(outline))
                } else {
                    pill.border_color(transparent_black())
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
                    .children(zoom_mark(zoomed, &probe, text, cx))
                    .children(mute_mark(muted, &probe, text, cx)),
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
            .px(px(MARGIN_X))
            .py(px(MARGIN_Y))
            .flex_none()
            .flex()
            .cursor_pointer()
            .child(pill)
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

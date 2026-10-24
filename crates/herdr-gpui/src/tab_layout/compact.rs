//! Icons only, for narrow groups and many tabs. Each tab is what runs in it,
//! with its status riding the icon's corner; the chosen tab also names itself
//! and carries its close button, and the rest name themselves on hover.

use super::{
    DOT_GAP, FLOATING_NEW_TAB_WIDTH, FLOATING_PADDING, TabContext, TabFace, TabLayout, TabShape,
    TabState, ZOOM_ICON, close_button, floating_new_tab, kind_icon, status_mark, zoom_mark,
};
use crate::{
    config::{FontConfig, Theme, corners, mix},
    sidebar::Indicators,
};
use gpui::{prelude::*, *};

/// Space around each tab's chip, inside the tab, so chips never touch.
const MARGIN_X: f32 = 1.;
const MARGIN_Y: f32 = 4.;

/// The padding inside a chip on either side.
const PAD: f32 = 6.;

const ICON: f32 = 13.;

/// The gap between the chosen chip's icon, title, and close button.
const GAP: f32 = 6.;

const CLOSE: f32 = 16.;

/// Where the status sits over the icon's top right corner.
const STATUS_TOP: f32 = 2.;
const STATUS_LEFT: f32 = PAD + ICON - 5.;

pub(super) struct Compact;

impl TabLayout for Compact {
    fn width(&self, title: f32, shape: TabShape, _: &FontConfig, _: Indicators) -> f32 {
        let icon = 2. * MARGIN_X + PAD + ICON + PAD;
        if !shape.selected {
            return icon;
        }
        let zoom = if shape.zoomed {
            DOT_GAP + ZOOM_ICON
        } else {
            0.
        };
        icon + GAP + title + zoom + GAP + CLOSE
    }

    fn colors(&self, state: TabState, theme: &Theme) -> (u32, u32) {
        match (state.selected, state.active) {
            (true, true) => {
                let fill = theme.primary_wash();
                (fill, theme.text_on(fill))
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
        let theme = cx.theme;
        let wash = mix(theme.surface, theme.active, 45);
        let status = status_mark(&mark, &probe, cx).map(|status| {
            div()
                .absolute()
                .top(px(STATUS_TOP))
                .left(px(STATUS_LEFT))
                .p(px(1.))
                .rounded_full()
                .bg(rgb(background))
                .child(status)
        });
        let chip = div()
            .relative()
            .h_full()
            .flex()
            .items_center()
            .gap(px(GAP))
            .px(px(PAD))
            .rounded(px(corners::CONTROL))
            .text_color(rgb(text))
            .map(|chip| {
                if state.selected {
                    chip.bg(cx.fill.over(theme.surface, background))
                } else {
                    chip.hover(move |s| s.bg(rgb(wash)))
                }
            })
            .child(kind_icon(&mark, &probe, ICON, text, cx))
            .when(state.selected, |chip| {
                chip.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(DOT_GAP))
                        .child(label.clone())
                        .children(zoom_mark(zoomed, &probe, text, cx)),
                )
            })
            .children(status);
        // Only the chosen tab draws its close button; middle-click still
        // closes the rest.
        let chip = if state.selected {
            chip.child(close_button(close, &probe, CLOSE, text, cx))
        } else {
            chip
        };
        let (foreground, surface) = (theme.foreground, theme.active);
        base.px(px(MARGIN_X))
            .py(px(MARGIN_Y))
            .flex_none()
            .flex()
            .cursor_pointer()
            .when(!state.selected, |tab| {
                tab.tooltip(move |_, cx| {
                    cx.new(|_| crate::menu::SettingsHint {
                        text: label.clone(),
                        foreground,
                        surface,
                    })
                    .into()
                })
            })
            .child(chip)
    }

    fn min_width(&self) -> f32 {
        2. * MARGIN_X + PAD + ICON + PAD
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

//! Curved tabs' panes: each pane a rounded card on the chrome, with a gutter
//! between. The pane in use has a faint ring; its title is a small chip in a
//! lip above the cells, so it never covers output.

use super::{PaneChrome, PaneContext, PaneFace, column};
use crate::config::{corners, mix};
use gpui::{prelude::*, *};

/// A card's corners.
const CARD: f32 = 10.;
const GUTTER: f32 = 6.;
const LIP: f32 = 5.;
const CHIP_MAX: f32 = 260.;

pub(crate) struct Curved;

impl PaneChrome for Curved {
    fn gutter(&self) -> f32 {
        GUTTER
    }

    fn inset(&self) -> f32 {
        GUTTER
    }

    fn veil(&self) -> u8 {
        0x38
    }

    fn corners(&self) -> (f32, f32) {
        // The lip above carries the top corners; inside the 1px border.
        (0., CARD - 1.)
    }

    fn pane(&self, face: PaneFace, cells: AnyElement, cx: &PaneContext<'_>) -> AnyElement {
        let theme = cx.theme;
        let background = theme.background;
        let chip = cx
            .title_text()
            .max_w(px(CHIP_MAX))
            .px_2()
            .py(px(2.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(mix(background, theme.active, 70)))
            .text_size(px(cx.tabs.size - 2.))
            .text_color(rgb(if face.active {
                theme.foreground
            } else {
                theme.subtext()
            }))
            .child(cx.dot(face.status))
            .child(div().min_w_0().truncate().child(face.title));
        let lip = div()
            .flex_none()
            .flex()
            .justify_end()
            .pt(px(LIP))
            .px(px(LIP))
            .rounded_t(px(CARD - 1.))
            .bg(rgb(background))
            .child(chip);
        column()
            .rounded(px(CARD))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(if face.active {
                mix(theme.active, theme.primary(), 30)
            } else {
                background
            }))
            .child(lip)
            .child(cells)
            .into_any_element()
    }
}

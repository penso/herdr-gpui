//! Classic tabs' panes: square cells under a thin header row like a small
//! classic tab. The pane in use takes the chosen tab's wash; the others dim.

use super::{PaneChrome, PaneContext, PaneFace, column};
use gpui::{prelude::*, *};

const HEADER: f32 = 22.;

pub(crate) struct Classic;

impl PaneChrome for Classic {
    fn gutter(&self) -> f32 {
        1.
    }

    fn veil(&self) -> u8 {
        0x50
    }

    fn pane(&self, face: PaneFace, cells: AnyElement, cx: &PaneContext<'_>) -> AnyElement {
        let theme = cx.theme;
        let (fill, text) = match face.active {
            true => {
                let wash = theme.primary_wash();
                (wash, theme.text_on(wash))
            }
            false => (theme.surface, theme.muted),
        };
        let header = cx
            .title_text()
            .flex_none()
            .h(px(HEADER))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .bg(rgb(fill))
            .text_color(rgb(text))
            .child(cx.dot(face.status))
            .child(div().flex_1().min_w_0().truncate().child(face.title))
            .when_some(face.size, |header, size| {
                header.child(div().flex_none().text_color(rgb(theme.muted)).child(size))
            });
        column().child(header).child(cells).into_any_element()
    }
}

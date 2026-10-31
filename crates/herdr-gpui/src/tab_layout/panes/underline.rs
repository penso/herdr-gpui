//! Underline tabs' panes: no headers. Each pane ends in a one-line status
//! bar, as an editor's or tmux's: program, folder, state, and size. The pane
//! in use has its bar in the accent wash.

use super::{PaneChrome, PaneContext, PaneFace, column};
use crate::fonts::StyledFont;
use gpui::{prelude::*, *};

const BAR: f32 = 20.;
const TITLE_MAX: f32 = 160.;

pub(crate) struct Underline;

impl PaneChrome for Underline {
    fn gutter(&self) -> f32 {
        1.
    }

    fn veil(&self) -> u8 {
        0x50
    }

    fn pane(&self, face: PaneFace, cells: AnyElement, cx: &PaneContext<'_>) -> AnyElement {
        let theme = cx.theme;
        let face_font = cx.terminal;
        let (fill, text) = match face.active {
            true => {
                let wash = theme.primary_wash();
                (wash, theme.text_on(wash))
            }
            false => (theme.surface, theme.muted),
        };
        let state = cx.state(face.status);
        let bar = div()
            .flex_none()
            .h(px(BAR))
            .px_2()
            .flex()
            .items_center()
            .gap_3()
            .bg(rgb(fill))
            .text_font(face_font)
            .text_size(px(face_font.size - 2.))
            .text_color(rgb(text))
            .child(
                div()
                    .flex_none()
                    .max_w(px(TITLE_MAX))
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(face.title),
            )
            .child(div().flex_1().min_w_0().truncate().children(face.folder))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(cx.dot(face.status))
                    .children(state),
            )
            .children(face.size.map(|size| div().flex_none().child(size)));
        column().child(cells).child(bar).into_any_element()
    }
}

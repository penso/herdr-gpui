//! Pill tabs' panes: rounded panes nearly edge to edge, each with a pill
//! label floating in its bottom corner. Panes not in use take a soft veil
//! instead of a header. A label fades away while the pointer is over its
//! pane, so it never hides the output being read.

use super::{PaneChrome, PaneContext, PaneFace, column};
use crate::config::corners;
use gpui::{prelude::*, *};

/// A pane's corners.
const PANE: f32 = corners::CONTROL;
const LABEL: f32 = 20.;
const LABEL_MAX: f32 = 260.;
const LABEL_INSET: f32 = 8.;
/// The hover group a pane names, so its label can step aside.
const GROUP: &str = "core-pane-pill";

pub(crate) struct Pills;

impl PaneChrome for Pills {
    fn gutter(&self) -> f32 {
        2.
    }

    fn gutter_fill(&self, theme: &crate::config::Theme) -> u32 {
        theme.surface
    }

    fn veil(&self) -> u8 {
        0x70
    }

    fn corners(&self) -> (f32, f32) {
        (PANE, PANE)
    }

    fn pane(&self, face: PaneFace, cells: AnyElement, cx: &PaneContext<'_>) -> AnyElement {
        let theme = cx.theme;
        let state = cx.state(face.status);
        let label = cx
            .title_text()
            .absolute()
            .bottom(px(LABEL_INSET))
            .right(px(LABEL_INSET))
            .max_w(px(LABEL_MAX))
            .h(px(LABEL))
            .px_2()
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded_full()
            .bg(rgb(if face.active {
                theme.primary_wash()
            } else {
                theme.surface
            }))
            .text_size(px(cx.tabs.size - 2.))
            .text_color(rgb(if face.active {
                theme.foreground
            } else {
                theme.muted
            }))
            .group_hover(GROUP, |label| label.opacity(0.))
            .child(cx.dot(face.status))
            .child(div().min_w_0().truncate().child(face.title))
            .when_some(state, |label, state| {
                label.child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.subtext()))
                        .child(state),
                )
            });
        column()
            .group(GROUP)
            .relative()
            .rounded(px(PANE))
            .overflow_hidden()
            .child(cells)
            .child(label)
            .into_any_element()
    }
}

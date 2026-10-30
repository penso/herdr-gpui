//! Shared headings for the sidebar's workspace and agent sections.

use crate::{
    config::{FontConfig, Theme},
    direction::Logical,
    sidebar::{label_text, layout::SidebarLook, line_height},
};
use gpui::{prelude::*, *};

pub(in crate::sidebar) fn header(
    label: &'static str,
    font: &FontConfig,
    theme: &Theme,
    look: SidebarLook,
) -> Div {
    div()
        .debug_selector(|| format!("header-{label}"))
        .flex_none()
        .h(px(line_height(font) + 2. * look.density.header_padding()))
        .px(px(look.content_x()))
        .row()
        .items_center()
        .text_size(px(font.size))
        .text_color(rgb(theme.muted))
        .child(
            div()
                .debug_selector(|| format!("header-label-{label}"))
                .child(label_text(&look.header_label(label))),
        )
}

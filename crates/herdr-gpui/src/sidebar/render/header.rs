//! Shared headings for the sidebar's workspace and agent sections.

use crate::{
    config::{FontConfig, Theme},
    direction::Logical,
    sidebar::{label_text, layout::SidebarLook, line_height},
};
use gpui::{prelude::*, *};

/// A section's heading: `key` names its debug selectors, `label` is its
/// text, cased as the look's headers are.
pub(in crate::sidebar) fn header(
    key: &'static str,
    label: &str,
    font: &FontConfig,
    theme: &Theme,
    look: SidebarLook,
) -> Div {
    div()
        .debug_selector(|| format!("header-{key}"))
        .flex_none()
        .h(px(line_height(font) + 2. * look.density.header_padding()))
        .px(px(look.content_x()))
        .row()
        .items_center()
        .text_size(px(font.size))
        .text_color(rgb(theme.muted))
        .child(
            div()
                .debug_selector(|| format!("header-label-{key}"))
                .child(label_text(&look.header_label(label))),
        )
}

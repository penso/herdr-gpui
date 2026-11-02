//! The sidebar's resizable edges: its width on the end edge, and the split
//! between the spaces and agents sections.
use super::super::SidebarDrag;
use crate::{HerdrWindow, config::Theme, direction::Logical, panel_resize::Grip};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The line between the spaces and agents sections, dragged to share the
    /// height between them; a double-click shares it evenly again.
    pub(super) fn sidebar_split_edge(&self, theme: &Theme, cx: &mut Context<Self>) -> Grip {
        Grip::new(
            Axis::Vertical,
            div()
                .id("sidebar-split-resize")
                .debug_selector(|| "sidebar-split-resize".into())
                .h(px(6.))
                .flex_none()
                .cursor(CursorStyle::ResizeUpDown)
                .border_t_1()
                .border_color(rgb(theme.active))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.sidebar_split_modified = true;
                        if event.click_count == 2 {
                            this.sidebar_drag = None;
                            this.sidebar_split = None;
                            this.save_chrome();
                        } else {
                            this.sidebar_drag = Some(SidebarDrag::Split);
                        }
                        cx.notify();
                    }),
                ),
        )
        .dragging(matches!(self.sidebar_drag, Some(SidebarDrag::Split)))
    }

    /// The sidebar's end edge, dragged from `width` to resize it; a
    /// double-click returns it to its default width.
    pub(super) fn sidebar_width_edge(&self, width: f32, cx: &mut Context<Self>) -> Grip {
        Grip::new(
            Axis::Horizontal,
            div()
                .id("sidebar-resize")
                .debug_selector(|| "sidebar-resize".into())
                .absolute()
                .inset_e(px(0.))
                .top_0()
                .h_full()
                .w(px(6.))
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.sidebar_modified = true;
                        if event.click_count == 2 {
                            this.sidebar_drag = None;
                            this.sidebar_width = None;
                            this.save_sidebar_width();
                        } else {
                            this.sidebar_drag = Some(SidebarDrag::Width {
                                start: f32::from(event.position.x),
                                width,
                            });
                        }
                        cx.notify();
                    }),
                ),
        )
        .dragging(matches!(self.sidebar_drag, Some(SidebarDrag::Width { .. })))
    }
}

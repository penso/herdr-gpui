//! The sidebar as a cached child view.
//!
//! Terminal output redraws the window many times a second, and rebuilding and
//! laying out every workspace and agent row each time cost more than painting
//! the terminal. GPUI reuses a cached view's layout and paint until that view
//! is notified, so the window notifies this one whenever it is notified itself
//! (`HerdrWindow::new` observes itself), and a surface-only update redraws the
//! window without notifying it (`HerdrWindow::redraw_terminal`).
//!
//! The rows still come from `HerdrWindow::render_sidebar`, so their listeners
//! and state stay where they were.

use super::metrics::sidebar_extent;
use crate::preferences::SidebarMode;
use crate::window::HerdrWindow;
use gpui::{
    AnyView, Context, Empty, Entity, IntoElement, Render, StyleRefinement, Styled, ViewElement,
    WeakEntity, Window, px,
};

pub(crate) struct SidebarView {
    window: WeakEntity<HerdrWindow>,
    #[cfg(test)]
    pub(crate) renders: usize,
}

impl SidebarView {
    pub(crate) fn new(window: WeakEntity<HerdrWindow>) -> Self {
        Self {
            window,
            #[cfg(test)]
            renders: 0,
        }
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        #[cfg(feature = "integration-test")]
        {
            cx.default_global::<crate::performance::Counts>()
                .sidebar_renders += 1;
        }
        // A closed window leaves nothing to draw.
        self.window
            .update(cx, |view, cx| {
                view.render_sidebar(window, cx).into_any_element()
            })
            .unwrap_or_else(|_| Empty.into_any_element())
    }
}

/// The sidebar in the window body. The outer style repeats the sidebar's own
/// root, which GPUI lays out without rendering while the cache holds.
pub(crate) fn cached(
    view: &Entity<SidebarView>,
    mode: SidebarMode,
    preferred: Option<f32>,
    window_width: f32,
) -> ViewElement<AnyView> {
    let width = sidebar_extent(mode, preferred, window_width);
    AnyView::from(view.clone()).cached(
        StyleRefinement::default()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0(),
    )
}

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

use crate::window::HerdrWindow;
use gpui_kit::{
    AnyView, Context, Empty, Entity, IntoElement, Render, StyleRefinement, Styled, WeakEntity,
    Window,
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

/// The sidebar in its resizable panel, which decides its width.
pub(crate) fn cached(view: &Entity<SidebarView>) -> gpui_kit::ViewElement<AnyView> {
    AnyView::from(view.clone()).cached(StyleRefinement::default().size_full().min_h_0())
}

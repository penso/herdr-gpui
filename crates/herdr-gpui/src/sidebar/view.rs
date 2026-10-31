//! The sidebar as a cached child view.
//!
//! Terminal output redraws the window many times a second, and rebuilding and
//! laying out every workspace and agent row each time cost more than painting
//! the terminal. GPUI reuses a cached view's layout and paint until that view
//! is notified, so the window notifies this one whenever it is notified itself
//! (`HerdrWindow::new` observes itself), and a surface-only update redraws the
//! window without notifying it (`HerdrWindow::redraw_terminal`). Moving
//! marks, the status marks and the activity bar's busy pulse, draw in the
//! window's mark layer (`mark_layer`), so their frames do not invalidate it
//! either.
//!
//! The rows still come from `HerdrWindow::render_sidebar`, so their listeners
//! and state stay where they were.

use super::{SidebarMode, agents::Indicators};
use crate::window::HerdrWindow;
use gpui::{
    AnyView, Context, Empty, Entity, IntoElement, Render, StyleRefinement, Styled, ViewElement,
    WeakEntity, Window, px,
};

pub(crate) struct SidebarView {
    window: WeakEntity<HerdrWindow>,
    indicators: Indicators,
    #[cfg(test)]
    pub(crate) renders: usize,
}

impl SidebarView {
    pub(crate) fn new(window: WeakEntity<HerdrWindow>, indicators: Indicators) -> Self {
        Self {
            window,
            indicators,
            #[cfg(test)]
            renders: 0,
        }
    }

    pub(crate) fn set_indicators(&mut self, indicators: Indicators, cx: &mut Context<Self>) {
        if self.indicators != indicators {
            self.indicators = indicators;
            cx.notify();
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
        // Every moving mark is reserved again below.
        if let Some(marks) = crate::mark_layer::Marks::of(window, cx) {
            marks.forget(cx.entity_id());
        }
        // A closed window leaves nothing to draw.
        self.window
            .update(cx, |view, cx| match view.sidebar_mode() {
                _ if view.activity_bar_leads() => view
                    .render_activity(self.indicators, window, cx)
                    .into_any_element(),
                SidebarMode::Rail => view
                    .render_rail(self.indicators, window, cx)
                    .into_any_element(),
                _ => view
                    .render_sidebar(self.indicators, window, cx)
                    .into_any_element(),
            })
            .unwrap_or_else(|_| Empty.into_any_element())
    }
}

/// The sidebar in the window body, `width` wide as its mode lays it out. The
/// outer style repeats the sidebar's own root, which GPUI lays out without
/// rendering while the cache holds.
pub(crate) fn cached(view: &Entity<SidebarView>, width: f32) -> ViewElement<AnyView> {
    AnyView::from(view.clone()).cached(
        StyleRefinement::default()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0(),
    )
}

#[cfg(test)]
mod tests;

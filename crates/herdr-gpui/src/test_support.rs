//! Headless window fixtures that match production: the kit is initialized and
//! the view sits inside a kit `Root`, so dialogs, notifications and tooltips
//! have the layers they render into.

use gpui_kit::{
    App, AppContext as _, Context, Entity, Render, TestAppContext, VisualTestContext, Window,
    component::{Root, theme::Theme},
};

/// Initializes the kit once per test app, as `app::run` does at startup.
pub(crate) fn init_kit(cx: &mut App) {
    if !cx.has_global::<Theme>() {
        gpui_kit::init(cx);
    }
}

/// `TestAppContext::add_window_view`, with the view wrapped in a kit `Root`.
pub(crate) fn add_window_view<V: Render + 'static>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
) -> (Entity<V>, &mut VisualTestContext) {
    cx.update(init_kit);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| build(window, cx));
        Root::new(view, window, cx)
    });
    let view = root.read_with(cx, |root, _| root.view().clone().downcast::<V>());
    match view {
        Ok(view) => (view, cx),
        Err(_) => unreachable!("the root was built around this view type"),
    }
}

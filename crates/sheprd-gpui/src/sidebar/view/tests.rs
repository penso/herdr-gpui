#![allow(clippy::unwrap_used)]

use super::*;
use crate::{herdr_settings::Settings, sidebar::layout_tests::fixture_window};

#[gpui::test]
fn shared_indicator_reload_invalidates_cache_but_identical_props_do_not(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let sidebar = cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        view.read(cx).sidebar_view.clone()
    });
    for text in [
        "[theme.custom]\nyellow = '#123456'\n",
        "[theme.custom]\nyellow = '#ff9900'\n",
        "[theme.custom]\nyellow = '#00ff00'\n",
    ] {
        let (before, previous) = cx.update(|_, cx| {
            let sidebar = sidebar.read(cx);
            (sidebar.renders, sidebar.indicators)
        });
        view.update(cx, |view, cx| {
            view.settings.shared = Some(Settings::parse_text(text).unwrap());
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (renders, indicators) = cx.update(|_, cx| {
            let parent = view.read(cx);
            let expected = Indicators::new(
                parent.settings.shared.as_ref(),
                parent.config.status_icons,
                parent.config.edge_rail,
                matches!(
                    cx.window_appearance(),
                    gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight
                ),
                &parent.theme,
            );
            let sidebar = sidebar.read(cx);
            assert_eq!(sidebar.indicators, expected);
            assert_ne!(sidebar.indicators, previous);
            assert!(sidebar.renders > before);
            (sidebar.renders, sidebar.indicators)
        });
        sidebar.update(cx, |sidebar, cx| sidebar.set_indicators(indicators, cx));
        view.update(cx, |view, cx| view.redraw_terminal(cx));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(sidebar.read(cx).renders, renders);
        });
    }
}

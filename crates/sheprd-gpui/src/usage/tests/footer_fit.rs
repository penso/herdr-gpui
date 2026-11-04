use super::*;
use crate::config::status_bar::Detail;
use crate::sidebar::layout_tests::{fixture_window, full_draw};
use gpui::{Bounds, Pixels, TestAppContext, px, size};

const SERVICES: [&str; 6] = ["grok", "claude", "codex", "copilot", "zed", "cursor"];

const SELECTORS: [&str; 6] = [
    "usage-grok",
    "usage-claude",
    "usage-codex",
    "usage-copilot",
    "usage-zed",
    "usage-cursor",
];

fn readings() -> Vec<Reading> {
    SERVICES
        .iter()
        .enumerate()
        .map(|(index, id)| Reading {
            provider: provider(id),
            report: Some(report(provider(id), 90. - index as f64 * 10.)),
            error: None,
            access: None,
        })
        .collect()
}

fn visible(cx: &mut gpui::VisualTestContext) -> Vec<&'static str> {
    let row = cx.debug_bounds("usage").unwrap();
    SERVICES
        .into_iter()
        .zip(SELECTORS)
        .filter(|(_, selector)| {
            cx.debug_bounds(selector)
                .is_some_and(|segment| contained(row, segment))
        })
        .map(|(id, _)| id)
        .collect()
}

fn contained(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left()
        && inner.right() <= outer.right()
        && inner.top() >= outer.top()
        && inner.bottom() <= outer.bottom()
}

#[gpui::test]
fn the_footer_fits_more_than_two_and_follows_resize_in_both_directions(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.config.usage.show = true;
        view.usage = super::super::Usage::showing(super::super::Host::Local, readings());
        view
    });
    cx.simulate_resize(size(px(1800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(visible(cx), SERVICES);

    cx.simulate_resize(size(px(1000.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let narrow = visible(cx);
    assert!(!narrow.is_empty());
    assert!(narrow.len() < SERVICES.len());
    assert_eq!(narrow, SERVICES[..narrow.len()]);
    let row = cx.debug_bounds("usage").unwrap();
    for selector in SELECTORS {
        let segment = cx.debug_bounds(selector).unwrap();
        // A lower-priority provider wraps wholly below the visible row.
        assert!(contained(row, segment) || segment.top() >= row.bottom());
    }
    let bar = cx.debug_bounds("connection-status").unwrap();
    for id in [
        "usage-refresh",
        "status-theme",
        "status-keybinds",
        "report-issue",
        "status-version",
    ] {
        assert!(contained(bar, cx.debug_bounds(id).unwrap()), "{id}");
    }

    view.update(cx, |view, cx| {
        view.usage.choose(provider("codex"));
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(visible(cx).contains(&"codex"));
    let selected = cx.debug_bounds("usage-codex").unwrap();
    assert_eq!(selected.left(), cx.debug_bounds("usage").unwrap().left());

    view.update(cx, |view, cx| {
        view.config.status_bar.usage = Detail::Compact;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(visible(cx).len() > narrow.len());

    cx.simulate_resize(size(px(1800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(visible(cx), SERVICES);
    let position = cx.debug_bounds("usage-cursor").unwrap().center();
    cx.simulate_mouse_down(
        position,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(
        position,
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let view = view.read(cx);
        assert_eq!(
            view.menu.page,
            Some(crate::menu::Page::Usage(provider("cursor")))
        );
        assert!(view.menu.focus.is_focused(window));
    });
}

#![allow(clippy::unwrap_used)]
use super::*;
use crate::{sidebar::layout_tests::full_draw, titlebar::tests::header_window};
use core::prelude::v1::test;
use gpui::{Modifiers, TestAppContext, VisualTestContext, size};
use herdr_client::protocol::ClientShellSnapshot;
use std::sync::Arc;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(header_window);
    cx.simulate_resize(size(px(1200.), px(600.)));
    draw(cx);
    (view, cx)
}

const PANES: &[&str] = &["a", "b", "c"];

/// The selected endpoint's daemon, with `PANES` open, focuses each of
/// `panes` in turn.
fn visit(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, panes: &[&str]) {
    let fixture: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.panes = PANES
                .iter()
                .map(|id| {
                    let mut pane = fixture.panes[0].clone();
                    pane.pane_id = (*id).to_owned();
                    pane
                })
                .collect();
            for pane in panes {
                snapshot.focused_pane_id = Some((*pane).to_owned());
                let snapshot = Arc::new(snapshot.clone());
                view.live.snapshot = Some(snapshot.clone());
                let endpoint = &mut view.endpoints[view.selected_endpoint];
                endpoint.live.snapshot = Some(snapshot);
                endpoint.sync_live();
            }
            cx.notify();
        })
    });
    draw(cx);
}

fn can(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (bool, bool) {
    view.read_with(cx, |view, _| {
        (view.can_travel(Step::Back), view.can_travel(Step::Forward))
    })
}

#[gpui::test]
fn back_and_forward_follow_the_sidebar_toggle(cx: &mut TestAppContext) {
    let (_, cx) = window(cx);
    let bar = cx.debug_bounds("titlebar").unwrap();
    let toggle = cx.debug_bounds("toggle-sidebar").unwrap();
    let back = cx.debug_bounds("titlebar-back").unwrap();
    let forward = cx.debug_bounds("titlebar-forward").unwrap();
    let (button, _) = Style::NATIVE.button();
    for control in [back, forward] {
        assert_eq!(control.size, button);
        assert!(bar.contains(&control.origin) && bar.contains(&control.bottom_right()));
        // Level with the toggle, whatever height the platform's buttons take.
        assert!((control.center().y - toggle.center().y).abs() <= px(1.));
    }
    assert!(back.left() >= toggle.right());
    assert!(forward.left() >= back.right());
    // Close enough to read as one group with the toggle.
    assert!(back.left() - toggle.right() <= px(12.));
    // A window too narrow for them keeps the account reachable instead.
    cx.simulate_resize(size(px(240.), px(600.)));
    draw(cx);
    assert!(cx.debug_bounds("titlebar-navigation").is_none());
    assert!(cx.debug_bounds("toggle-sidebar").is_some());
}

#[gpui::test]
fn the_buttons_enable_as_the_trail_grows(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    assert_eq!(can(&view, cx), (false, false));
    // A press on a disabled button neither travels nor moves the window.
    let back = cx.debug_bounds("titlebar-back").unwrap();
    cx.simulate_click(back.center(), Modifiers::default());
    assert_eq!(can(&view, cx), (false, false));
    visit(&view, cx, &["a", "b"]);
    assert_eq!(can(&view, cx), (true, false));
    // Without a connection the daemon is never asked, so the trail stays put
    // rather than waiting on a focus that cannot arrive.
    cx.simulate_click(back.center(), Modifiers::default());
    assert_eq!(can(&view, cx), (true, false));
}

#[test]
fn each_platform_draws_its_own_buttons() {
    let native = if cfg!(target_os = "macos") {
        Style::Segmented
    } else if cfg!(windows) {
        Style::Fluent
    } else {
        Style::Adwaita
    };
    assert_eq!(Style::NATIVE, native);
    // Fluent uses arrows; the others turn one chevron around.
    assert_eq!(Style::Fluent.icon(Step::Back), "icons/arrow-left.svg");
    for style in [Style::Segmented, Style::Adwaita] {
        assert_eq!(style.icon(Step::Back), style.icon(Step::Forward));
    }
    // Every style fits the 34 px header with room to spare.
    for style in [Style::Segmented, Style::Fluent, Style::Adwaita] {
        assert!(style.button().0.height <= px(28.));
    }
}

#[gpui::test]
fn a_press_that_cannot_be_sent_leaves_the_landing_step_alone(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    visit(&view, cx, PANES);
    // A first Back to "b" was queued and is still landing.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let selected = view.selected_endpoint;
            view.endpoints[selected].history.begin(1);
        })
    });
    // A second press the window cannot send (no connection here) must not
    // replace or cancel it.
    cx.update(|_, cx| view.update(cx, |view, cx| view.travel(Step::Back, cx)));
    visit(&view, cx, &["b"]);
    // "b" landed as a step back, so Forward still returns to "c".
    assert_eq!(can(&view, cx), (true, true));
}

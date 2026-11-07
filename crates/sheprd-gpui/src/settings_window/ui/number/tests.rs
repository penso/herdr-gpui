#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::Theme;
use core::prelude::v1::test;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn parse_number_accepts_integers_inside_the_band_only() {
    let band = 0.0..=48.0;
    assert_eq!(parse_number(" 14 ", &band), Some(14.));
    assert_eq!(parse_number("0", &band), Some(0.));
    assert_eq!(parse_number("48", &band), Some(48.));
    assert_eq!(parse_number("49", &band), None);
    assert_eq!(parse_number("-1", &band), None);
    assert_eq!(parse_number("1.5", &band), None);
    assert_eq!(parse_number("", &band), None);
    assert_eq!(parse_number("abc", &band), None);
}

#[test]
fn slider_value_maps_pointer_to_the_band_and_rounds() {
    let bounds = Bounds::new(point(px(100.), px(0.)), size(px(200.), px(16.)));
    let band = 0.0..=48.0;
    assert_eq!(slider_value(px(100.), bounds, &band), 0.);
    assert_eq!(slider_value(px(300.), bounds, &band), 48.);
    assert_eq!(slider_value(px(200.), bounds, &band), 24.);
    assert_eq!(slider_value(px(50.), bounds, &band), 0.);
    assert_eq!(slider_value(px(400.), bounds, &band), 48.);
}

#[test]
fn stepping_rounds_first_clamps_and_rejects_non_finite_values() {
    let band = 8.0..=48.0;
    assert_eq!(stepped_value(8., -1., &band), Some(8.));
    assert_eq!(stepped_value(48., 1., &band), Some(48.));
    assert_eq!(stepped_value(14.2, 1., &band), Some(15.));
    assert_eq!(stepped_value(13.5, -1., &band), Some(13.));
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(stepped_value(invalid, 1., &band), None);
        assert_eq!(stepped_value(14., invalid, &band), None);
    }
}

type Log = Rc<RefCell<Vec<NumberChange>>>;

struct Harness {
    look: Look,
    enabled: bool,
    seen: Log,
    other: Log,
    edits: Rc<Cell<u32>>,
}

impl Harness {
    fn new() -> (Self, Log, Log, Rc<Cell<u32>>) {
        let (seen, other, edits) = (Log::default(), Log::default(), Rc::new(Cell::new(0)));
        let harness = Self {
            look: Look::Slider,
            enabled: true,
            seen: seen.clone(),
            other: other.clone(),
            edits: edits.clone(),
        };
        (harness, seen, other, edits)
    }
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default();
        let (seen, other, edits) = (self.seen.clone(), self.other.clone(), self.edits.clone());
        let spec = |id, look, enabled| Number {
            id,
            value: 14.,
            range: 0.0..=48.0,
            unit: Unit::Pixels,
            look,
            enabled,
        };
        div()
            .size_full()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(number(
                &theme,
                spec("n", self.look, self.enabled),
                None,
                move |change, _, _| seen.borrow_mut().push(change),
                move |_, _| edits.set(edits.get() + 1),
            ))
            .child(number(
                &theme,
                spec("m", Look::Slider, true),
                None,
                move |change, _, _| other.borrow_mut().push(change),
                |_, _| {},
            ))
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn center(cx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.debug_bounds(selector).unwrap().center()
}

fn drag(cx: &mut VisualTestContext, id: &'static str, release_below: bool) {
    let slider = cx.debug_bounds(id).unwrap();
    let y = slider.center().y;
    let start = point(slider.left() + px(1.), y);
    let end = point(slider.right() - px(1.), y);
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    // The first move past the threshold only starts the drag; the next one moves it.
    cx.simulate_mouse_move(
        point(start.x + px(20.), y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    let up = if release_below {
        point(end.x, y + px(300.))
    } else {
        end
    };
    cx.simulate_mouse_up(up, MouseButton::Left, Default::default());
    cx.run_until_parked();
}

fn commits(log: &Log) -> usize {
    log.borrow()
        .iter()
        .filter(|c| matches!(c, NumberChange::Commit(_)))
        .count()
}

#[gpui::test]
fn steppers_commit_and_slider_drag_previews_then_commits_once(cx: &mut TestAppContext) {
    let (harness, seen, _, _) = Harness::new();
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    for selector in ["n-plus", "n-minus"] {
        let at = center(cx, selector);
        cx.simulate_click(at, Default::default());
    }
    cx.run_until_parked();
    assert!(matches!(
        seen.borrow()[..],
        [NumberChange::Commit(15.), NumberChange::Commit(13.)]
    ));
    seen.borrow_mut().clear();
    drag(cx, "n-slider", false);
    let seen = seen.borrow();
    assert!(
        seen.iter().any(|c| matches!(c, NumberChange::Preview(_))),
        "{seen:?}"
    );
    assert!(
        matches!(seen.last(), Some(NumberChange::Commit(48.))),
        "{seen:?}"
    );
    assert_eq!(
        seen.iter()
            .filter(|c| matches!(c, NumberChange::Commit(_)))
            .count(),
        1
    );
}

#[gpui::test]
fn a_drag_released_outside_the_slider_still_commits_once(cx: &mut TestAppContext) {
    let (harness, seen, _, _) = Harness::new();
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    drag(cx, "n-slider", true);
    assert_eq!(commits(&seen), 1, "{seen:?}");
    assert!(matches!(
        seen.borrow().last(),
        Some(NumberChange::Commit(48.))
    ));
}

#[gpui::test]
fn a_press_that_never_becomes_a_drag_emits_nothing(cx: &mut TestAppContext) {
    let (harness, seen, other, _) = Harness::new();
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    let at = center(cx, "n-slider");
    cx.simulate_mouse_down(at, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(
        seen.borrow().is_empty() && other.borrow().is_empty(),
        "{seen:?}"
    );
}

#[gpui::test]
fn dragging_one_control_never_reaches_another(cx: &mut TestAppContext) {
    let (harness, seen, other, _) = Harness::new();
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    drag(cx, "m-slider", false);
    assert_eq!(commits(&other), 1, "{other:?}");
    assert!(seen.borrow().is_empty(), "{seen:?}");
}

#[gpui::test]
fn a_disabled_control_emits_nothing(cx: &mut TestAppContext) {
    let (mut harness, seen, _, edits) = Harness::new();
    harness.enabled = false;
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    for selector in ["n-plus", "n-minus", "n-field", "n-value"] {
        if cx.debug_bounds(selector).is_some() {
            let at = center(cx, selector);
            cx.simulate_click(at, Default::default());
        }
    }
    drag(cx, "n-slider", false);
    assert!(seen.borrow().is_empty(), "{seen:?}");
    assert_eq!(edits.get(), 0);
}

#[gpui::test]
fn clicking_the_value_asks_to_edit(cx: &mut TestAppContext) {
    let (harness, seen, _, edits) = Harness::new();
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    let field = cx.debug_bounds("n-field").unwrap();
    cx.simulate_click(
        point(field.left() + px(10.), field.center().y),
        Default::default(),
    );
    cx.run_until_parked();
    assert_eq!(edits.get(), 1);
    assert!(seen.borrow().is_empty());
}

#[gpui::test]
fn an_abandoned_drag_leaves_no_commit_behind(cx: &mut TestAppContext) {
    let (harness, seen, _, _) = Harness::new();
    let (view, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    let slider = cx.debug_bounds("n-slider").unwrap();
    let y = slider.center().y;
    let start = point(slider.left() + px(1.), y);
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(start.x + px(20.), y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(slider.right() - px(1.), y),
        MouseButton::Left,
        Default::default(),
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|c| matches!(c, NumberChange::Preview(_)))
    );
    // The slider disappears mid-drag, then comes back.
    view.update(cx, |harness, cx| {
        harness.look = Look::Steppers;
        cx.notify();
    });
    draw(cx);
    // The release lands where no slider listens, so the drag just ends.
    let away = point(px(5.), px(5.));
    cx.simulate_mouse_up(away, MouseButton::Left, Default::default());
    seen.borrow_mut().clear();
    view.update(cx, |harness, cx| {
        harness.look = Look::Slider;
        cx.notify();
    });
    draw(cx);
    for selector in ["n-plus", "n-minus"] {
        let at = center(cx, selector);
        cx.simulate_click(at, Default::default());
    }
    cx.run_until_parked();
    // The first release after it tells the owner to drop its preview,
    // never to write the stale value.
    assert!(
        matches!(
            seen.borrow()[..],
            [
                NumberChange::Abandon,
                NumberChange::Commit(15.),
                NumberChange::Commit(13.)
            ]
        ),
        "{seen:?}"
    );
}

#[gpui::test]
fn a_bare_field_shows_only_its_value(cx: &mut TestAppContext) {
    let (mut harness, _, _, edits) = Harness::new();
    harness.look = Look::Field;
    let (_, cx) = cx.add_window_view(|_, _| harness);
    draw(cx);
    for selector in ["n-plus", "n-minus", "n-slider"] {
        assert!(cx.debug_bounds(selector).is_none(), "{selector}");
    }
    let at = center(cx, "n-field");
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
    assert_eq!(edits.get(), 1);
}

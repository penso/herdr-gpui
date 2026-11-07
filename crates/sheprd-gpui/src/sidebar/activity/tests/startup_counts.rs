//! The bar counts pull requests and issues from the start: the sections'
//! view is made by the tick, not by the first click on a section.
use super::*;
use crate::sidebar::activity::tests::sections::{local, show};
use gpui::{Entity, VisualTestContext};

fn tick(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_orchestrators(window, cx)));
}

fn views(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| view.read(cx).orchestrators.len())
}

#[gpui::test]
fn the_tick_makes_the_sections_view_without_a_click(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    assert_eq!(views(&view, cx), 0);
    tick(&view, cx);
    assert_eq!(views(&view, cx), 1);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.activity.panel, ActivityPanel::Workspaces);
        assert!(view.activity.section_view.is_some());
    });
    // Later ticks, and then showing a section, keep the one view.
    tick(&view, cx);
    show(&view, cx, ActivityPanel::Issues);
    assert_eq!(views(&view, cx), 1);
}

#[gpui::test]
fn no_view_is_made_while_the_bar_is_off(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.config.sidebar_style.activity_bar = false);
    });
    tick(&view, cx);
    assert_eq!(views(&view, cx), 0);
}

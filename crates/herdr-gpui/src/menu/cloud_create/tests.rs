#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    cloud::{CloudProvider, Spec},
    control::batch::Phase,
    sidebar::layout_tests::fixture_window,
};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};

mod en_xa;

fn creation(provider: CloudProvider, count: u32) -> Creation {
    Creation {
        provider,
        count,
        spec: Spec::new(),
        setup: true,
    }
}

fn ask(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext, Registry, u64) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ask_to_create_cloud(creation(CloudProvider::ALL[0], 3), window, cx)
        })
    });
    let registry = cx.update(|_, cx| cx.default_global::<Registry>().clone());
    (view, cx, registry, id.unwrap())
}

#[gpui::test]
fn the_question_says_how_many_and_what_comes_with_them(cx: &mut TestAppContext) {
    let (view, cx, registry, id) = ask(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::CreateCloud));
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("create-cloud-cost").is_some());
    assert!(cx.debug_bounds("create-cloud-submit").is_some());
    let batch = registry.get(id).unwrap();
    assert_eq!(batch.phase, Phase::Confirming);
    assert_eq!(batch.kind, BatchKind::Create);
    assert_eq!(batch.devices.len(), 3);
    assert!(batch.devices[2].label.ends_with(" 3"), "{batch:?}");
}

#[gpui::test]
fn closing_the_question_creates_nothing(cx: &mut TestAppContext) {
    let (view, cx, registry, id) = ask(cx);
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    assert_eq!(registry.get(id).unwrap().phase, Phase::Cancelled);
}

#[test]
fn the_count_and_choices_read_plainly() {
    // Without Fluent's isolation marks around each name and number.
    let plain = |creation: &Creation| title(creation).replace(['\u{2068}', '\u{2069}'], "");
    let mut one = creation(CloudProvider::ALL[0], 1);
    let name = crate::cloud::name(one.provider);
    assert!(plain(&one).starts_with(&format!("Create 1 {name} ")));
    one.count = 4;
    assert!(plain(&one).starts_with(&format!("Create 4 {name} ")));
    assert!(plain(&one).ends_with("s?"));
    one.spec.shape = Shape::Sandbox;
    assert!(plain(&one).ends_with(" sandboxes?"));
    let lines = choices(&one);
    assert!(lines.iter().any(|line| line.contains("Herdr is installed")));
    assert!(lines.iter().any(|line| line.contains("setup script runs")));
    one.spec.install = false;
    one.setup = false;
    let lines = choices(&one);
    assert!(lines.iter().any(|line| line.contains("not installed")));
    assert!(lines.iter().any(|line| line == "No setup script runs."));
}

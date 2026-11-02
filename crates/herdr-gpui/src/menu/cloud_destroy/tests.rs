#![allow(clippy::unwrap_used)]
use super::*;
use crate::{cloud::CloudProvider, control::batch::Phase, sidebar::layout_tests::fixture_window};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};

mod en_xa;

fn target(provider: CloudProvider, id: &str) -> Target {
    Target {
        provider,
        id: id.into(),
        endpoint: format!("{}:{id}", provider.key()),
        label: format!("Box {id}"),
    }
}

fn ask(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext, Registry, u64) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let targets: Vec<_> = CloudProvider::ALL
        .iter()
        .enumerate()
        .map(|(index, &provider)| target(provider, &format!("m{index}")))
        .collect();
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ask_to_destroy_cloud(&targets, window, cx)
        })
    });
    let registry = cx.update(|_, cx| cx.default_global::<Registry>().clone());
    (view, cx, registry, id.unwrap())
}

#[gpui::test]
fn the_question_names_every_device_and_what_happens_to_it(cx: &mut TestAppContext) {
    let (view, cx, registry, id) = ask(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::DestroyCloud));
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    for index in 0..CloudProvider::ALL.len() {
        assert!(
            cx.debug_bounds(format!("destroy-cloud-device-{index}").leak())
                .is_some()
        );
    }
    assert!(cx.debug_bounds("destroy-cloud-submit").is_some());
    assert_eq!(registry.get(id).unwrap().phase, Phase::Confirming);
}

#[gpui::test]
fn escape_cancels_the_batch(cx: &mut TestAppContext) {
    let (view, cx, registry, id) = ask(cx);
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    assert_eq!(registry.get(id).unwrap().phase, Phase::Cancelled);
}

#[gpui::test]
fn cancel_cancels_the_batch(cx: &mut TestAppContext) {
    let (view, cx, registry, id) = ask(cx);
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let cancel = cx.debug_bounds("destroy-cloud-cancel").unwrap().center();
    cx.simulate_click(cancel, Modifiers::default());
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    assert_eq!(registry.get(id).unwrap().phase, Phase::Cancelled);
}

#[gpui::test]
fn another_page_holds_the_window_so_nothing_is_asked(cx: &mut TestAppContext) {
    let (view, cx, registry, _) = ask(cx);
    let before = registry.all().len();
    let again = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ask_to_destroy_cloud(&[target(CloudProvider::ALL[0], "x")], window, cx)
        })
    });
    assert!(again.is_none());
    assert_eq!(registry.all().len(), before, "no batch is opened");
}

#[test]
fn each_row_says_whether_the_machine_is_deleted_or_only_forgotten() {
    for &provider in CloudProvider::ALL {
        // Without Fluent's isolation marks around each name.
        let text = consequence(&target(provider, "m")).replace(['\u{2068}', '\u{2069}'], "");
        let name = crate::cloud::name(provider);
        if crate::cloud::deletion(provider).is_some() {
            assert!(text.starts_with("Delete the "), "{text}");
            assert!(text.contains(&format!(" at {name}")), "{text}");
            assert!(text.ends_with(" and remove it from Herdr"), "{text}");
        } else {
            assert!(text.starts_with("Remove it from Herdr only"), "{text}");
        }
    }
    let plain = |count: usize| {
        fl!("cloud-destroy-submit", count = count).replace(['\u{2068}', '\u{2069}'], "")
    };
    assert_eq!(plain(1), "Destroy 1 device");
    assert_eq!(plain(3), "Destroy 3 devices");
}

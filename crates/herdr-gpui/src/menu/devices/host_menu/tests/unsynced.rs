//! Before deleting a machine at its provider, the Remove dialog searches it
//! for work pushed nowhere and, when it finds some, names it and asks again.

use super::*;
use crate::HerdrWindow;
use crate::cloud::{
    CloudProvider,
    unsynced::{Checkout, FAKE},
};
use gpui::Entity;

fn open_remove<'a>(
    cx: &'a mut TestAppContext,
    id: &str,
    header: &'static str,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let endpoint = format!("azure:{id}");
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.endpoints[0].collapsed = true;
        view.endpoints.push(crate::endpoint::Endpoint::new(
            endpoint.clone(),
            "Build box".into(),
            herdr_client::ConnectTarget::Cloud {
                provider: CloudProvider::Azure,
                account: "00000000-0000-0000-0000-000000000000".into(),
                id: id.into(),
                machine: id.into(),
                session: "default".into(),
            },
            true,
        ));
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let header = cx.debug_bounds(header).unwrap();
    cx.simulate_mouse_down(header.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(header.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_keystrokes("up enter");
    cx.run_until_parked();
    (view, cx)
}

#[gpui::test]
fn work_pushed_nowhere_is_named_and_delete_asks_again(cx: &mut TestAppContext) {
    FAKE.lock().unwrap().get_or_insert_default().insert(
        "herdr-dirty".into(),
        vec![Checkout {
            path: "~/code/app".into(),
            changed: 2,
            unpushed: 1,
        }],
    );
    let (view, cx) = open_remove(cx, "herdr-dirty", "host-azure:herdr-dirty");
    view.read_with(cx, |view, _| {
        let host = host(view);
        assert_eq!(super::super::remove_label(host), "Delete anyway");
        let text = super::super::destroy::unsynced_text(&host.unsynced, "VM")
            .unwrap()
            .replace(['\u{2068}', '\u{2069}'], "");
        assert!(
            text.contains("~/code/app (2 changed files, 1 unpushed commit)"),
            "{text}"
        );
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("remove-device-unsynced").is_some());
}

#[gpui::test]
fn a_clean_machine_is_deleted_without_a_second_question(cx: &mut TestAppContext) {
    FAKE.lock()
        .unwrap()
        .get_or_insert_default()
        .insert("herdr-clean".into(), Vec::new());
    let (view, cx) = open_remove(cx, "herdr-clean", "host-azure:herdr-clean");
    view.read_with(cx, |view, _| {
        assert_eq!(
            super::super::remove_label(host(view)).replace(['\u{2068}', '\u{2069}'], ""),
            "Delete VM"
        );
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("remove-device-unsynced").is_none());
}

#[gpui::test]
fn a_machine_that_cannot_be_searched_is_said_to_be_unchecked(cx: &mut TestAppContext) {
    let (view, cx) = open_remove(cx, "herdr-unreachable", "host-azure:herdr-unreachable");
    view.read_with(cx, |view, _| {
        let host = host(view);
        let text = super::super::destroy::unsynced_text(&host.unsynced, "VM")
            .unwrap()
            .replace(['\u{2068}', '\u{2069}'], "");
        assert!(text.starts_with("Could not check the VM"), "{text}");
        assert_eq!(
            super::super::remove_label(host).replace(['\u{2068}', '\u{2069}'], ""),
            "Delete VM"
        );
    });
}

#[gpui::test]
fn delete_waits_for_the_search_to_answer(cx: &mut TestAppContext) {
    let (view, cx) = open_remove(cx, "herdr-slow", "host-azure:herdr-slow");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let host = view.menu.host.as_mut().unwrap();
            host.unsynced = super::super::destroy::Unsynced::Checking {
                _cancel: super::super::destroy::Cancel::for_test(),
            };
            assert_eq!(super::super::remove_label(host), "Checking…");
            view.confirm_remove_device(window, cx);
            assert!(view.menu.host.is_some(), "nothing is removed yet");
            assert!(view.menu.removing_devices.is_empty());
        })
    });
}

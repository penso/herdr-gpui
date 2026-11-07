#![allow(clippy::unwrap_used)]
use super::{Listing, MachinesPage, Name, Settings, gigabytes};
use crate::settings_window::{Section, SettingsWindow, tests::skill_fixture};
use gpui::{TestAppContext, VisualTestContext};

fn listing(name: &str, saved: bool) -> Listing {
    Listing {
        name: Name::try_from(name.to_owned()).unwrap(),
        settings: Settings::default(),
        saved,
        disk_bytes: 25_300_000_000,
    }
}

/// Settings on this page, showing `listings`.
fn page<'a>(
    cx: &'a mut TestAppContext,
    listings: Vec<Listing>,
    confirming: Option<&str>,
) -> (gpui::Entity<SettingsWindow>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(gpui::size(gpui::px(960.), gpui::px(2000.)));
    view.update(cx, |view, _| {
        view.section = Section::Machines;
        view.machines = MachinesPage {
            listings: Some(listings),
            confirming: confirming.map(|name| Name::try_from(name.to_owned()).unwrap()),
            ..MachinesPage::default()
        };
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    (view, cx)
}

#[test]
fn disk_use_reads_in_decimal_gigabytes_as_the_finder_counts() {
    assert_eq!(gigabytes(0), "0.0");
    assert_eq!(gigabytes(25_300_000_000), "25.3");
}

#[gpui::test]
fn each_machine_shows_its_settings_and_a_delete_button(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx, vec![listing("one", false), listing("two", false)], None);
    for selector in [
        "machine-one-cpus-more",
        "machine-one-cpus-less",
        "machine-one-memory-more",
        "machine-one-open",
        "machine-one-delete",
        "machine-two-cpus-more",
        "machine-two-delete",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
}

#[gpui::test]
fn delete_asks_a_second_time_before_anything_is_lost(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx, vec![listing("one", false)], Some("one"));
    assert!(cx.debug_bounds("machine-one-delete-confirm").is_some());
    assert!(cx.debug_bounds("machine-one-delete-cancel").is_some());
    assert!(cx.debug_bounds("machine-one-delete").is_none());
}

#[gpui::test]
fn only_the_named_machine_is_asked_about(cx: &mut TestAppContext) {
    let machines = vec![listing("one", false), listing("two", false)];
    let (_view, cx) = page(cx, machines, Some("two"));
    assert!(cx.debug_bounds("machine-one-delete").is_some());
    assert!(cx.debug_bounds("machine-one-delete-confirm").is_none());
    assert!(cx.debug_bounds("machine-two-delete-confirm").is_some());
}

#[gpui::test]
fn a_saved_machine_offers_to_forget_its_state(cx: &mut TestAppContext) {
    let (_view, cx) = page(cx, vec![listing("one", true)], None);
    assert!(cx.debug_bounds("machine-one-forget").is_some());
}

#[gpui::test]
fn cancelling_keeps_the_machine_and_its_button(cx: &mut TestAppContext) {
    let (view, cx) = page(cx, vec![listing("one", false)], Some("one"));
    view.update(cx, |view, cx| {
        view.machines.confirming = None;
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("machine-one-delete").is_some());
}

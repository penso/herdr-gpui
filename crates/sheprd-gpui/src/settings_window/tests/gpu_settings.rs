//! The GPU's two switches: the status bar's and the sidebar's, each found by
//! searching for "gpu" and each saving its own preference.
use super::*;
use crate::config::preferences::Preference;
use gpui::{px, size};
use std::sync::{Arc, Mutex};

#[test]
fn searching_gpu_finds_the_status_bar_and_sidebar_rows() {
    let ids: Vec<_> = catalog::search("gpu")
        .into_iter()
        .map(|index| catalog::CATALOG[index].id)
        .collect();
    assert!(ids.contains(&"status-gpu"), "{ids:?}");
    assert!(ids.contains(&"sidebar-gpu-load"), "{ids:?}");
    assert!(catalog::search("video memory").len() >= 2);
}

#[gpui::test]
fn each_gpu_switch_saves_its_own_preference(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    view.update(cx, |view, _| {
        let captured = edits.clone();
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
            move |edit| captured.lock().unwrap().push(edit),
        ));
    });
    cx.simulate_resize(size(px(960.), px(2000.)));
    for (section, selector, expected) in [
        (
            Section::StatusBar,
            "status-gpu-toggle",
            Preference::ShowGpuLoad(true),
        ),
        (
            Section::Sidebar,
            "sidebar-gpu-load-toggle",
            Preference::ShowSidebarGpuLoad(true),
        ),
    ] {
        view.update(cx, |view, cx| {
            view.section = section;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let point = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector}"))
            .center();
        cx.simulate_click(point, Default::default());
        cx.run_until_parked();
        assert_eq!(edits.lock().unwrap().last(), Some(&expected), "{selector}");
    }
}

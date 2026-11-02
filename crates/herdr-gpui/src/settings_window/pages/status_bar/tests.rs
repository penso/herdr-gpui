#![allow(clippy::unwrap_used)]

use super::*;
use crate::settings_window::{Section, controls::preferences::PreferenceIo, tests::skill_fixture};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext, px, size};
use std::sync::{Arc, Mutex};

#[gpui::test]
fn status_bar_controls_save_typed_edits(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, _| {
        view.section = Section::StatusBar;
        view.controls.preference_io = Some(PreferenceIo::recording(move |edit| {
            captured.lock().unwrap().push(edit)
        }));
    });
    cx.simulate_resize(size(px(960.), px(2200.)));
    let click = |cx: &mut VisualTestContext, selector: &'static str| {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let point = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(point, Default::default());
        cx.run_until_parked();
        edits.lock().unwrap().pop()
    };
    for (selector, edit) in [
        ("status-usage-detail-1", Edit::Usage(Detail::Compact)),
        (
            "status-system-load-detail-1",
            Edit::SystemLoad(Detail::Compact),
        ),
        ("status-keep-awake-toggle", Edit::KeepAwake(false)),
        ("status-theme-toggle", Edit::Theme(Button::Hidden)),
        ("status-theme-style-1", Edit::Theme(Button::Icon)),
        ("status-shortcuts-style-1", Edit::Shortcuts(Button::Icon)),
        (
            "status-report-issue-toggle",
            Edit::ReportIssue(Button::Hidden),
        ),
    ] {
        assert_eq!(
            click(cx, selector),
            Some(Preference::StatusBar(edit)),
            "{selector}"
        );
    }
    assert_eq!(
        click(cx, "status-system-load-toggle"),
        Some(Preference::ShowSystemLoad(false))
    );
    assert_eq!(
        click(cx, "status-gpu-toggle"),
        Some(Preference::ShowGpuLoad(true))
    );
    // The current choice, and the choices of a hidden item, save nothing.
    assert_eq!(click(cx, "status-usage-detail-0"), None);
    // The GPU is off by default, so its detail waits until it is shown.
    assert_eq!(click(cx, "status-gpu-detail-1"), None);
    view.update(cx, |view, cx| {
        view.config.show_gpu_load = true;
        cx.notify();
    });
    assert_eq!(
        click(cx, "status-gpu-detail-1"),
        Some(Preference::StatusBar(Edit::GpuDetail(Detail::Compact)))
    );
    view.update(cx, |view, cx| {
        view.config.status_bar.theme = Button::Hidden;
        cx.notify();
    });
    assert_eq!(click(cx, "status-theme-style-1"), None);
    assert_eq!(
        click(cx, "status-theme-toggle"),
        Some(Preference::StatusBar(Edit::Theme(Button::Label)))
    );
}

/// The choices name messages, which `fl!` cannot check.
#[test]
fn choices_name_messages() {
    let none: &[&str] = &[];
    let messages = DETAILS
        .iter()
        .map(|(_, message)| *message)
        .chain(SCOPES.iter().map(|(_, message)| *message))
        .chain(STYLES.iter().map(|(_, message)| *message));
    for message in messages {
        assert_eq!(i18n::takes(message), Some(none), "{message}");
    }
}

use super::*;
use crate::config::preferences::Preference;
use crate::herdr_settings::TabBarPosition;
use crate::settings_window::inputs::NumberField;
use std::sync::{Arc, Mutex};

fn recording(view: &mut SettingsWindow) -> Arc<Mutex<Vec<Preference>>> {
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
        move |edit| captured.lock().unwrap().push(edit),
    ));
    edits
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn show(view: &Entity<SettingsWindow>, section: Section, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.section = section;
        cx.notify();
    });
    draw(cx);
}

#[gpui::test]
fn behaviour_pages_route_every_row_to_its_preference(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = view.update(cx, |view, _| recording(view));
    cx.simulate_resize(size(px(960.), px(2000.)));
    let delay = view.read_with(cx, |view, _| view.config.notifications.delay_seconds);
    for (section, selector, expected) in [
        (
            Section::StatusBar,
            "status-system-load-toggle",
            Preference::ShowSystemLoad(false),
        ),
        (
            Section::StatusBar,
            "status-network-quality-toggle",
            Preference::ShowNetworkQuality(true),
        ),
        (
            Section::General,
            "general-ports-toggle",
            Preference::ShowListeningPorts(false),
        ),
        (
            Section::General,
            "general-checkpoints-toggle",
            Preference::AgentCheckpoints(false),
        ),
        (
            Section::Tabs,
            "tabs-confirm-tab-toggle",
            Preference::ConfirmCloseTab(false),
        ),
        (
            Section::Tabs,
            "tabs-confirm-pane-toggle",
            Preference::ConfirmClosePane(false),
        ),
        (
            Section::Notifications,
            "notif-delay-control-plus",
            Preference::NotificationDelay(Some(delay + 1)),
        ),
    ] {
        show(&view, section, cx);
        let point = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector}"))
            .center();
        cx.simulate_click(point, Default::default());
        cx.run_until_parked();
        assert_eq!(edits.lock().unwrap().last(), Some(&expected), "{selector}");
    }
}

#[gpui::test]
fn override_dropdowns_offer_the_shared_value_first(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = view.update(cx, |view, _| recording(view));
    cx.simulate_resize(size(px(960.), px(2000.)));
    show(&view, Section::Notifications, cx);
    let button = cx.debug_bounds("notif-position-control").unwrap().center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("notif-position-control-option-0").is_some(),
        "dropdown open"
    );
    let option = cx
        .debug_bounds("notif-position-control-option-0")
        .unwrap()
        .center();
    cx.simulate_click(option, Default::default());
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::NotificationPosition(None)),
        "first option follows Herdr"
    );
    show(&view, Section::Notifications, cx);
    let button = cx.debug_bounds("notif-position-control").unwrap().center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    draw(cx);
    let option = cx
        .debug_bounds("notif-position-control-option-2")
        .unwrap()
        .center();
    cx.simulate_click(option, Default::default());
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::NotificationPosition(Some(
            herdr_client::protocol::ToastHerdrPosition::TopRight
        )))
    );
}

#[gpui::test]
fn a_failed_preference_write_reports_and_reloads(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::failing());
    });
    cx.simulate_resize(size(px(960.), px(2000.)));
    show(&view, Section::Tabs, cx);
    let toggle = cx.debug_bounds("tabs-confirm-tab-toggle").unwrap().center();
    cx.simulate_click(toggle, Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.error.is_some());
        assert!(!view.busy());
    });
}

#[gpui::test]
fn dropdown_keys_move_pick_and_escape(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = view.update(cx, |view, _| recording(view));
    cx.simulate_resize(size(px(960.), px(2000.)));
    cx.update(|window, _| window.activate_window());
    show(&view, Section::Notifications, cx);
    for (keys, expected) in [
        ("escape", None),
        (
            "down enter",
            Some(Preference::NotificationPosition(Some(
                herdr_client::protocol::ToastHerdrPosition::TopLeft,
            ))),
        ),
    ] {
        let button = cx.debug_bounds("notif-position-control").unwrap().center();
        cx.simulate_click(button, Default::default());
        cx.run_until_parked();
        draw(cx);
        assert!(view.read_with(cx, |view, _| view.dropdown.is_some()));
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.dropdown.is_none(), "{keys}"));
        assert_eq!(edits.lock().unwrap().last(), expected.as_ref(), "{keys}");
        draw(cx);
    }
}

#[gpui::test]
fn the_delay_field_commits_valid_numbers_and_holds_invalid_ones(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = view.update(cx, |view, _| recording(view));
    cx.simulate_resize(size(px(960.), px(2000.)));
    cx.update(|window, _| window.activate_window());
    show(&view, Section::Notifications, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::ToastDelay, 1., window, cx)
        })
    });
    cx.simulate_input("4000");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.number_edit.as_ref().unwrap().invalid);
    });
    assert!(edits.lock().unwrap().is_empty());
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.number_edit.is_none()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::ToastDelay, 1., window, cx)
        })
    });
    cx.simulate_input("30");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::NotificationDelay(Some(30)))
    );
}

#[gpui::test]
fn folder_size_fields_commit_valid_numbers_and_hold_invalid_ones(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = view.update(cx, |view, _| recording(view));
    cx.simulate_resize(size(px(960.), px(2000.)));
    cx.update(|window, _| window.activate_window());
    show(&view, Section::General, cx);
    // Raise the cap so 4000 MB is a legal warning.
    view.update(cx, |view, _| {
        view.config.remote_files.folder_transfer_cap_mb = 4096
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FolderWarningMb, 64., window, cx)
        })
    });
    cx.simulate_input("4000");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::FolderWarningMb(4000))
    );

    // The warning may not land above the cap the window shows.
    view.update(cx, |view, _| {
        view.config.remote_files.folder_transfer_cap_mb = 512
    });
    let before = edits.lock().unwrap().len();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FolderWarningMb, 64., window, cx)
        })
    });
    cx.simulate_input("4000");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(edits.lock().unwrap().len(), before);
    view.read_with(cx, |view, _| {
        assert_eq!(view.config.remote_files.folder_warning_mb, 64);
    });

    // Zero is outside the field's range, so the field stays open and invalid.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FolderWarningMb, 64., window, cx)
        })
    });
    cx.simulate_input("0");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.number_edit.as_ref().unwrap().invalid);
    });
    assert_eq!(edits.lock().unwrap().len(), before);
    cx.simulate_keystrokes("escape");
}

#[gpui::test]
fn shared_rows_say_why_they_are_disabled(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3000.)));
    for (section, ids) in [
        (Section::General, &["general-copy-on-select-reason"][..]),
        (
            Section::Tabs,
            &[
                "tabs-position-reason",
                "tabs-hide-single-reason",
                "tabs-pane-history-reason",
            ],
        ),
        (
            Section::Notifications,
            &["notif-delivery-reason", "notif-sound-reason"],
        ),
    ] {
        show(&view, section, cx);
        for id in ids {
            assert!(cx.debug_bounds(id).is_some(), "{id}");
        }
    }
    view.update(cx, |view, _| {
        view.shared = Some(herdr_settings::Settings::parse_text("").unwrap());
    });
    show(&view, Section::Tabs, cx);
    let gate = view.read_with(cx, |view, _| view.shared_gate());
    if cfg!(unix) {
        // Loaded shared settings make the rows editable again.
        assert_eq!(gate, None);
        assert!(cx.debug_bounds("tabs-position-reason").is_none());
        assert!(cx.debug_bounds("tabs-pane-history-reason").is_none());
        assert!(cx.debug_bounds("tabs-pane-history-description").is_some());
    } else {
        // Elsewhere Herdr's file cannot be written, so loading it changes
        // nothing: the rows keep the platform's reason over their description.
        assert_eq!(gate, Some(pages::SHARED_READ_ONLY));
        assert!(cx.debug_bounds("tabs-position-reason").is_some());
        assert!(cx.debug_bounds("tabs-pane-history-reason").is_some());
        assert!(cx.debug_bounds("tabs-pane-history-description").is_none());
    }
}

#[gpui::test]
fn configuration_lists_files_and_reload(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(1400.)));
    show(&view, Section::Configuration, cx);
    for id in ["config-local", "config-shared", "config-reload"] {
        assert!(cx.debug_bounds(id).is_some(), "{id}");
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    for id in ["config-local-reveal", "config-local-open"] {
        assert!(cx.debug_bounds(id).is_some(), "{id}");
    }
    assert!(cx.debug_bounds("config-reload-button").is_some());
}

#[gpui::test]
fn shared_controls_hold_while_a_save_is_in_flight(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(2200.)));
    view.update(cx, |view, cx| {
        view.section = Section::Tabs;
        view.shared = Some(
            herdr_settings::Settings::parse_text(
                "[ui]\ncopy_on_select = false\ntab_bar_position = 'bottom'",
            )
            .unwrap(),
        );
        view.saving = true;
        cx.notify();
    });
    draw(cx);
    // The unselected segment is the Top one; the rows show the loaded values.
    for id in [
        "tabs-position-control-0",
        "tabs-hide-single-toggle",
        "tabs-pane-history-toggle",
    ] {
        let control = cx.debug_bounds(id).unwrap();
        cx.simulate_click(control.center(), Default::default());
        view.read_with(cx, |view, _| {
            assert!(view.save_completion.is_none(), "{id}");
            let shared = view.shared.as_ref().unwrap();
            assert!(!shared.copy_on_select);
            assert_eq!(shared.tab_bar_position, TabBarPosition::Bottom);
            assert!(!shared.hide_tab_bar_when_single_tab);
            assert!(!shared.pane_history);
        });
    }
    show(&view, Section::General, cx);
    let control = cx.debug_bounds("general-copy-on-select-toggle").unwrap();
    cx.simulate_click(control.center(), Default::default());
    view.read_with(cx, |view, _| {
        assert!(view.save_completion.is_none());
        assert!(!view.shared.as_ref().unwrap().copy_on_select);
    });
}

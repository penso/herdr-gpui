//! The Show activity bar switch reaches every open main window at once.
use super::*;
use crate::config::preferences::Preference;
use crate::sidebar::ActivityPanel;
use crate::sidebar::layout_tests::{fixture_window, full_draw};

fn main_window(cx: &mut TestAppContext) -> WindowHandle<SheprdWindow> {
    cx.add_window(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.sidebar_style.activity_bar = true;
        view
    })
}

/// Draws `handle` and reports which of the bar, a section and the sidebar it
/// laid out.
fn drawn(handle: WindowHandle<SheprdWindow>, cx: &mut TestAppContext) -> (bool, bool, bool) {
    let cx = &mut VisualTestContext::from_window(handle.into(), cx);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (
        cx.debug_bounds("activity-bar").is_some(),
        cx.debug_bounds("activity-section").is_some(),
        cx.debug_bounds("sidebar").is_some(),
    )
}

#[gpui::test]
fn the_switch_hides_and_restores_the_bar_in_every_main_window(cx: &mut TestAppContext) {
    let workspaces = main_window(cx);
    let pull_requests = main_window(cx);
    pull_requests
        .update(cx, |view, _, _| {
            view.activity.pick(ActivityPanel::PullRequests);
        })
        .unwrap();
    assert_eq!(drawn(workspaces, cx), (true, false, true));
    assert_eq!(drawn(pull_requests, cx), (true, true, false));

    let edits = Arc::new(Mutex::new(Vec::new()));
    // What the file holds, so the reload after each save reads the write back.
    let disk = Arc::new(Mutex::new(true));
    let (captured, written) = (edits.clone(), disk.clone());
    let io = controls::preferences::PreferenceIo::with_load(
        move |edit| {
            if let Preference::SidebarActivityBar(shown) = edit {
                *written.lock().unwrap() = shown;
            }
            captured.lock().unwrap().push(edit);
        },
        move || {
            let mut loaded = fixture();
            loaded.config.sidebar_style.activity_bar = *disk.lock().unwrap();
            Ok(loaded)
        },
    );
    let weak = cx.update(|cx| workspaces.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let settings = cx.add_window(move |_, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.controls.preference_io = Some(io);
        view.initialize_controls(cx);
        view.section = Section::Sidebar;
        view
    });
    let toggle = |cx: &mut TestAppContext| {
        let cx = &mut VisualTestContext::from_window(settings.into(), cx);
        cx.simulate_resize(size(px(960.), px(2200.)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let switch = cx.debug_bounds("sidebar-activity-bar-toggle").unwrap();
        cx.simulate_click(switch.center(), Default::default());
        cx.run_until_parked();
    };

    toggle(cx);
    assert_eq!(
        edits.lock().unwrap().pop(),
        Some(Preference::SidebarActivityBar(false))
    );
    // Both bars go, and the Pull requests window shows its terminals again.
    assert_eq!(drawn(workspaces, cx), (false, false, true));
    assert_eq!(drawn(pull_requests, cx), (false, false, true));

    toggle(cx);
    assert_eq!(
        edits.lock().unwrap().pop(),
        Some(Preference::SidebarActivityBar(true))
    );
    // The section picked before comes back with the bar.
    assert_eq!(drawn(workspaces, cx), (true, false, true));
    assert_eq!(drawn(pull_requests, cx), (true, true, false));
}

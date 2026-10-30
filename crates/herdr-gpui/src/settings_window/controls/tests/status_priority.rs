use super::*;
use crate::{
    config::{DEFAULT_STATUS_PRIORITY, preferences::Preference},
    settings_window::controls::preferences::PreferenceIo,
};
use herdr_client::protocol::AgentStatus;
use std::sync::{Arc, Mutex};

#[gpui::test]
fn priority_buttons_reorder_reset_and_keep_focus_on_the_moved_button(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    let saved = Arc::new(Mutex::new(vec![
        AgentStatus::Blocked,
        AgentStatus::Idle,
        AgentStatus::Done,
        AgentStatus::Working,
    ]));
    let stored = saved.clone();
    view.update(cx, |view, cx| {
        view.config.sidebar_style.status_priority = vec![
            AgentStatus::Blocked,
            AgentStatus::Idle,
            AgentStatus::Done,
            AgentStatus::Working,
        ];
        view.loaded_config.sidebar_style.status_priority =
            view.config.sidebar_style.status_priority.clone();
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                if let Preference::SidebarStatusPriority(priority) = &edit {
                    *saved.lock().unwrap() = priority.clone();
                }
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(move || {
                let mut config = skill_load()?;
                config.config.sidebar_style.status_priority = stored.lock().unwrap().clone();
                Ok(config)
            }),
        });
        view.section = Section::Sidebar;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(2200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));

    for id in [
        "sidebar-status-priority-up-blocked",
        "sidebar-status-priority-down-working",
    ] {
        let end_button = cx.debug_bounds(id).unwrap();
        cx.simulate_click(end_button.center(), Default::default());
        cx.run_until_parked();
    }
    assert_eq!(
        view.read_with(cx, |view, _| view
            .config
            .sidebar_style
            .status_priority
            .clone()),
        [
            AgentStatus::Blocked,
            AgentStatus::Idle,
            AgentStatus::Done,
            AgentStatus::Working,
        ]
    );

    let move_done_up = cx.debug_bounds("sidebar-status-priority-up-done").unwrap();
    cx.simulate_click(move_done_up.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view
            .config
            .sidebar_style
            .status_priority
            .clone()),
        [
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Idle,
            AgentStatus::Working,
        ]
    );
    cx.update(|window, cx| {
        let moved_button_focus = view.read(cx).status_priority_focus[4].clone();
        assert!(
            moved_button_focus.is_focused(window),
            "the moved Done up button keeps focus after rerender"
        );
    });

    let reset = cx.debug_bounds("sidebar-status-priority-reset").unwrap();
    cx.simulate_click(reset.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view
            .config
            .sidebar_style
            .status_priority
            .clone()),
        DEFAULT_STATUS_PRIORITY
    );
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [
            Preference::SidebarStatusPriority(vec![
                AgentStatus::Blocked,
                AgentStatus::Done,
                AgentStatus::Idle,
                AgentStatus::Working,
            ]),
            Preference::SidebarStatusPriority(DEFAULT_STATUS_PRIORITY.to_vec()),
        ]
    );
}

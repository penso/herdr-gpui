use super::*;
use crate::config::{AgentsPlacement, preferences::Preference};
use crate::settings_window::controls::preferences::PreferenceIo;
use std::sync::{Arc, Mutex};

#[gpui::test]
fn agents_placement_saves_the_picked_option_while_agents_show(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(skill_load),
        });
    });
    cx.simulate_resize(size(px(960.), px(2200.)));
    for (selector, placement, current) in [
        (
            "sidebar-agents-placement-control-1",
            AgentsPlacement::Nested,
            AgentsPlacement::Panel,
        ),
        (
            "sidebar-agents-placement-control-0",
            AgentsPlacement::Panel,
            AgentsPlacement::Nested,
        ),
    ] {
        view.update(cx, |view, cx| {
            view.section = Section::Sidebar;
            view.config.show_agents = true;
            view.config.agents_placement = current;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let option = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(option.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(
            edits.lock().unwrap().pop(),
            Some(Preference::AgentsPlacement(placement))
        );
    }
    // With agents hidden there is nothing to place: the choice is disabled.
    view.update(cx, |view, cx| {
        view.config.show_agents = false;
        view.config.agents_placement = AgentsPlacement::Panel;
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let option = cx
        .debug_bounds("sidebar-agents-placement-control-1")
        .unwrap();
    cx.simulate_click(option.center(), Default::default());
    cx.run_until_parked();
    assert!(edits.lock().unwrap().is_empty());
}

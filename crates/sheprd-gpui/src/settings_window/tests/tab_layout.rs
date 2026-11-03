//! Settings > Tabs picks the tab layout, as View > Tab Layout does.
use super::*;
use crate::config::{TabLayoutMode, preferences::Preference};
use std::sync::{Arc, Mutex};

#[gpui::test]
fn the_tabs_page_saves_each_tab_layout(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
            move |edit| captured.lock().unwrap().push(edit),
        ));
        view.section = Section::Tabs;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(2000.)));
    for (index, &mode) in TabLayoutMode::ALL.iter().enumerate() {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let selector: &'static str =
            Box::leak(format!("tabs-layout-control-{index}").into_boxed_str());
        let point = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector}"))
            .center();
        cx.simulate_click(point, Default::default());
        cx.run_until_parked();
        // The layout in use is already saved, so choosing it writes nothing.
        let expected = (mode != TabLayoutMode::default()).then_some(Preference::TabLayout(mode));
        assert_eq!(
            edits.lock().unwrap().last(),
            expected.as_ref(),
            "{selector}"
        );
    }
}

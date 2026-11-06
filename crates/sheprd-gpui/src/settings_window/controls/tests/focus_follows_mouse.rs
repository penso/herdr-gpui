use super::*;
use crate::config::preferences::Preference;
use crate::settings_window::controls::preferences::PreferenceIo;
use std::sync::{Arc, Mutex};

#[gpui::test]
fn focus_follows_mouse_switch_saves_the_opposite(cx: &mut TestAppContext) {
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
    for enabled in [false, true] {
        view.update(cx, |view, cx| {
            view.section = Section::Tabs;
            view.config.focus_follows_mouse = enabled;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let switch = cx.debug_bounds("tabs-focus-follows-mouse-toggle").unwrap();
        cx.simulate_click(switch.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(
            edits.lock().unwrap().pop(),
            Some(Preference::FocusFollowsMouse(!enabled))
        );
    }
}

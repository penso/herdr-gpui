use super::*;
use crate::config::preferences::Preference;
use crate::settings_window::controls::preferences::PreferenceIo;
use std::sync::{Arc, Mutex};

#[gpui::test]
fn show_automations_is_on_the_sidebar_page_and_saves_the_opposite(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    // What the file holds, so the reload after a save reads the write back.
    let file = Arc::new(Mutex::new(true));
    let (captured, written, read) = (edits.clone(), file.clone(), file.clone());
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                if let Preference::SidebarAutomations(shown) = edit {
                    *written.lock().unwrap() = shown;
                }
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(move || {
                let mut loaded = skill_load()?;
                loaded.config.sidebar_style.automations = *read.lock().unwrap();
                Ok(loaded)
            }),
        });
        view.section = Section::Sidebar;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(2200.)));
    for shown in [true, false] {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let switch = cx.debug_bounds("sidebar-automations-toggle").unwrap();
        cx.simulate_click(switch.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(
            edits.lock().unwrap().pop(),
            Some(Preference::SidebarAutomations(!shown))
        );
        view.update(cx, |view, _| {
            assert_eq!(
                view.config.sidebar_style.automations, !shown,
                "shown {shown}"
            );
        });
    }
}

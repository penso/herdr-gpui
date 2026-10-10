use super::*;
use crate::config::{UpdateChannel, preferences::Preference};
use crate::settings_window::controls::preferences::PreferenceIo;
use std::sync::{Arc, Mutex};

#[gpui::test]
fn beta_switch_is_in_general_and_saves_the_other_channel(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: skill_load,
        });
    });
    cx.simulate_resize(size(px(960.), px(3200.)));
    for (current, saved) in [
        (UpdateChannel::Stable, UpdateChannel::Beta),
        (UpdateChannel::Beta, UpdateChannel::Stable),
    ] {
        view.update(cx, |view, cx| {
            view.section = Section::General;
            view.config.updates.channel = current;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let switch = cx.debug_bounds("settings-update-beta").unwrap();
        cx.simulate_click(switch.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(
            edits.lock().unwrap().pop(),
            Some(Preference::UpdateChannel(saved))
        );
    }
}

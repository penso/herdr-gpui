use super::*;
use crate::{
    config::preferences::Preference,
    settings_window::{controls::preferences::PreferenceIo, ui},
};
use core::prelude::v1::test;
use std::sync::{Arc, Mutex};

fn width_io(edits: Arc<Mutex<Vec<Preference>>>) -> PreferenceIo {
    let written = edits.clone();
    PreferenceIo::with_load(
        move |edit| written.lock().unwrap().push(edit),
        move || {
            let mut loaded = skill_load()?;
            loaded.config.features.acp_chats = true;
            if let Some(Preference::ChatMaxWidth(width)) = edits.lock().unwrap().last() {
                loaded.config.chat.max_width = *width;
            }
            Ok(loaded)
        },
    )
}

#[gpui::test]
fn full_width_switch_and_typed_width_save_the_same_setting(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    view.update(cx, |view, cx| {
        view.section = Section::General;
        view.config.features.acp_chats = true;
        view.controls.preference_io = Some(width_io(edits.clone()));
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(3200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(
        cx.debug_bounds("general-chat-width-control-field")
            .is_some()
    );
    let toggle = cx
        .debug_bounds("general-chat-full-width-toggle")
        .unwrap()
        .center();
    cx.simulate_click(toggle, Default::default());
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().as_slice(),
        &[Preference::ChatMaxWidth(0)]
    );
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(
        cx.debug_bounds("general-chat-width-control-field")
            .is_none()
    );
    let toggle = cx
        .debug_bounds("general-chat-full-width-toggle")
        .unwrap()
        .center();
    cx.simulate_click(toggle, Default::default());
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::ChatMaxWidth(
            crate::config::chat::DEFAULT_MAX_WIDTH
        ))
    );
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let field = cx.debug_bounds("general-chat-width-control-field").unwrap();
    // The field also contains the unit and steppers; editing starts on its
    // 44 px numeric value, not at the center of the complete control.
    cx.simulate_click(
        point(field.left() + px(22.), field.center().y),
        Default::default(),
    );
    view.read_with(cx, |view, _| {
        assert!(
            view.number_edit
                .as_ref()
                .is_some_and(|edit| edit.field == NumberField::ChatMaxWidth)
        );
    });
    cx.simulate_input("1200");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::ChatMaxWidth(1200))
    );
    view.read_with(cx, |view, _| assert_eq!(view.config.chat.max_width, 1200));
}

#[gpui::test]
fn chat_width_preview_is_unwritten_and_invalid_typed_values_stay_open(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    view.update(cx, |view, cx| {
        view.section = Section::General;
        view.config.features.acp_chats = true;
        view.controls.preference_io = Some(width_io(edits.clone()));
        view.commit_number(
            NumberField::ChatMaxWidth,
            ui::NumberChange::Preview(1500.),
            cx,
        );
        assert_eq!(view.number_value(NumberField::ChatMaxWidth, 800.), 1500.);
        assert_eq!(view.config.chat.max_width, 800);
        view.commit_number(NumberField::ChatMaxWidth, ui::NumberChange::Abandon, cx);
        assert_eq!(view.number_value(NumberField::ChatMaxWidth, 800.), 800.);
    });
    assert!(edits.lock().unwrap().is_empty());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::ChatMaxWidth, 800., window, cx)
        })
    });
    cx.simulate_input("319");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(edits.lock().unwrap().is_empty());
    view.read_with(cx, |view, _| {
        assert!(view.number_edit.as_ref().is_some_and(|edit| edit.invalid))
    });
}

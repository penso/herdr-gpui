use super::*;
use core::prelude::v1::test;

#[gpui::test]
fn only_foreground_status_bar_settings_keep_their_current_source_eligible(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let other = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let (other_id, retained) = cx.update(|cx| {
        let (source_id, source_weak) = source
            .update(cx, |view, _, cx| {
                view.active = false;
                (cx.entity_id(), cx.weak_entity())
            })
            .unwrap();
        let (other_id, other_weak) = other
            .update(cx, |view, _, cx| {
                view.active = false;
                (cx.entity_id(), cx.weak_entity())
            })
            .unwrap();
        open_fixture(source_weak, cx);
        let settings = cx.global::<SettingsWindowHandle>().window.unwrap();
        settings
            .update(cx, |view, window, _| {
                view.section = Section::StatusBar;
                window.activate_window();
            })
            .unwrap();
        assert!(usage_source_active(source_id, cx));
        assert!(!usage_source_active(other_id, cx));

        settings
            .update(cx, |view, _, _| view.section = Section::General)
            .unwrap();
        assert!(!usage_source_active(source_id, cx));
        settings
            .update(cx, |view, _, _| view.section = Section::StatusBar)
            .unwrap();
        other
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        assert!(!usage_source_active(source_id, cx));

        settings
            .update(cx, |view, window, cx| {
                view.retarget_source(other_weak, cx);
                window.activate_window();
            })
            .unwrap();
        assert!(!usage_source_active(source_id, cx));
        assert!(usage_source_active(other_id, cx));

        // A detached save can keep the model alive after its window closes.
        let retained = cx
            .global::<SettingsWindowHandle>()
            .model
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap();
        settings
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        (other_id, retained)
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(!usage_source_active(other_id, cx));
        assert!(
            cx.global::<SettingsWindowHandle>()
                .model
                .as_ref()
                .unwrap()
                .upgrade()
                .is_some()
        );
    });
    drop(retained);
}

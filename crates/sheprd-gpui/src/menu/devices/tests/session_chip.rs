//! The hint on the chip that makes a device's sessions findable: it offers the
//! shortcut only where the shortcut leads.
use super::*;
use crate::Command;

/// `text` without Fluent's isolation marks around each name.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[gpui::test]
fn only_the_device_on_screen_offers_the_shortcut(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        let shortcut = view
            .config
            .keybindings
            .primary(Command::Sessions)
            .to_owned();
        assert!(!shortcut.is_empty());
        assert_eq!(view.selected_endpoint, 0);
        assert_eq!(
            plain(view.session_chip_hint(0, "Local")),
            format!("Sessions on Local ({shortcut})")
        );
        // The shortcut opens the device this window is on, not this one.
        assert_eq!(plain(view.session_chip_hint(1, "Box")), "Sessions on Box");
    });
}

#[gpui::test]
fn an_unbound_shortcut_leaves_the_plain_hint(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.config.keybindings = crate::keymap::Keymap::with_overrides(
            &[("sessions".into(), crate::keymap::Binding::Many(Vec::new()))].into(),
            &Default::default(),
            &crate::keymap::DaemonKeys::default(),
        )
        .unwrap();
        assert_eq!(
            plain(view.session_chip_hint(0, "Local")),
            "Sessions on Local"
        );
    });
}

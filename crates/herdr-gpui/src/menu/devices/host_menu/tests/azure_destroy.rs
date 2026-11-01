//! Removing an Azure device deletes its machine at Azure by default, with
//! Remove from Herdr only as the other answer; other devices are only removed.

use super::*;
use crate::cloud::CloudProvider;

#[gpui::test]
fn removing_an_azure_device_deletes_its_machine_unless_asked_not_to(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.endpoints[0].collapsed = true;
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "azure:herdr-vm".into(),
            "Build box".into(),
            herdr_client::ConnectTarget::Cloud {
                provider: CloudProvider::Azure,
                account: "00000000-0000-0000-0000-000000000000".into(),
                id: "herdr-vm".into(),
                machine: "herdr-vm".into(),
                session: "default".into(),
            },
            true,
        ));
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let header = cx.debug_bounds("host-azure:herdr-vm").unwrap();
    cx.simulate_mouse_down(header.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(header.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_keystrokes("up enter");
    assert!(view.read_with(cx, |view, _| view.menu.page == Some(Page::RemoveDevice)));
    // Deleting at Azure is what Remove does unless asked otherwise.
    view.read_with(cx, |view, _| {
        let host = host(view);
        assert!(host.destroy);
        assert_eq!(
            super::super::remove_label(host).replace(['\u{2068}', '\u{2069}'], ""),
            "Delete VM"
        );
        let note = super::super::removal_note(host).replace(['\u{2068}', '\u{2069}'], "");
        assert!(
            note.starts_with("Delete the VM Build box (herdr-vm) at Azure"),
            "{note}"
        );
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let only = cx.debug_bounds("remove-device-only").unwrap();
    let submit = cx.debug_bounds("remove-device-submit").unwrap();
    assert!(
        only.right() <= submit.left(),
        "Delete is the last, default answer"
    );
}

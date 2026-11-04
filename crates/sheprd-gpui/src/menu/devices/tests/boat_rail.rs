use super::*;
use crate::cloud::CloudProvider;
use gpui::{Modifiers, px, size};

#[gpui::test]
fn boat_waits_under_add_provider_whose_tile_opens_its_settings(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_add_device(window, cx)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    if crate::cloud::unavailable().is_some() {
        assert!(cx.debug_bounds("add-device-source-catalog").is_none());
        return;
    }
    if std::env::var_os("HERDR_BOAT_API_URL").is_some() {
        return;
    }
    // Not set up, Boat waits under Add provider, whose tile opens Settings.
    assert!(cx.debug_bounds("add-device-source-boat").is_none());
    let target = cx
        .debug_bounds("add-device-source-catalog")
        .unwrap()
        .center();
    cx.simulate_click(target, Modifiers::default());
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let target = cx.debug_bounds("add-provider-boat").unwrap().center();
    cx.simulate_click(target, Modifiers::default());
    cx.update(|_, cx| {
        let menu = &view.read(cx).menu;
        assert_eq!(menu.settings_opened, Some(CloudProvider::Boat));
        assert_eq!(menu.page, None, "the dialog closes for Settings");
    });
}

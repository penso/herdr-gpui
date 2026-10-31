//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the Nearby devices
//! list: while it searches and once it has found devices, it draws only
//! translated messages, except for the devices found and their sources'
//! names.
use super::*;
use crate::{
    i18n::leak_tests::{assert_translated, draw, under_en_xa},
    sidebar::layout_tests::fixture_window,
};
use gpui::{px, size};

/// The cloud providers the rail beside the form lists, by name.
fn providers() -> Vec<String> {
    #[cfg(feature = "cloud")]
    return crate::cloud::CloudProvider::ALL
        .iter()
        .map(|&provider| crate::cloud::name(provider).to_owned())
        .collect();
    #[cfg(not(feature = "cloud"))]
    Vec::new()
}

#[test]
fn nearby_devices_show_only_messages_under_en_xa() {
    under_en_xa("nearby devices en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(900.)));
        draw(cx);
        recorder.take();
        view.update_in(cx, |view, window, cx| {
            view.open_add_device(window, cx);
            view.start_device_discovery_with(form::report, cx);
        });
        form::settle(cx);
        draw(cx);
        let data: Vec<String> = [
            "m4max",
            "m4max.tail1.ts.net",
            "saved",
            "saved.tail1.ts.net",
            "m4max.local",
            "nas",
            "ssh://nas.local:2222",
            "Tailscale",
            "Bonjour",
            "default",
            "WSL",
        ]
        .map(str::to_owned)
        .into_iter()
        .chain(providers())
        .collect();
        assert_translated("Add Device > Nearby devices", &recorder.take(), &data);
    });
}

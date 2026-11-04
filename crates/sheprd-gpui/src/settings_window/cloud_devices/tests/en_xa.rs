//! The `en-XA` leak test for every provider's tab: Settings' own page test
//! sees only the first.
#![allow(clippy::unwrap_used)]

use super::super::super::{Section, SettingsWindow};
use crate::cloud::CloudProvider;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

fn unconfigure(config: &mut crate::config::Config) {
    #[cfg(feature = "azure")]
    {
        config.azure = Default::default();
    }
    #[cfg(feature = "boat")]
    {
        config.boat = Default::default();
    }
    #[cfg(feature = "coder")]
    {
        config.coder = Default::default();
    }
    #[cfg(feature = "daytona")]
    {
        config.daytona = Default::default();
    }
    #[cfg(feature = "opensandbox")]
    {
        config.opensandbox = Default::default();
    }
    #[cfg(feature = "sprites")]
    {
        config.sprites = Default::default();
    }
    let _ = config;
}

/// What the tabs show that is not a message: provider names, as tabs and
/// as card titles, and the values fields show as their placeholders.
fn data() -> Vec<String> {
    let names = CloudProvider::ALL
        .iter()
        .map(|&provider| crate::cloud::name(provider));
    let mut data: Vec<String> = names
        .clone()
        .map(str::to_owned)
        .chain(names.map(str::to_uppercase))
        .collect();
    // Size names, architectures, and the auto-stop field's config value.
    #[cfg(feature = "azure")]
    data.extend(
        crate::azure::sizes::VM_CHOICES
            .iter()
            .map(|&size| size.to_owned())
            .chain(["Arm".into(), "x64".into(), "30m".into()]),
    );
    // The auto-stop field shows its config value.
    #[cfg(feature = "boat")]
    data.extend([
        crate::boat::DEFAULT_API_URL.into(),
        "boat_…".into(),
        "never".into(),
    ]);
    #[cfg(feature = "coder")]
    data.extend([
        "https://coder.example.com".into(),
        "http://127.0.0.1:47823/callback".into(),
    ]);
    #[cfg(feature = "container")]
    data.extend([
        crate::container::setup::DEFAULT_IMAGE.into(),
        crate::container::setup::DEFAULT_CPUS.into(),
        crate::container::setup::DEFAULT_MEMORY.into(),
    ]);
    #[cfg(feature = "daytona")]
    data.extend([crate::daytona::DEFAULT_API_URL.into(), "dtn_…".into()]);
    #[cfg(feature = "opensandbox")]
    data.extend([
        crate::opensandbox::DEFAULT_API_URL.into(),
        "buildpack-deps:bookworm".into(),
        "4Gi".into(),
    ]);
    #[cfg(feature = "railway")]
    data.extend(["/Users/you/.ssh/id_ed25519".into(), "herdr-railway".into()]);
    #[cfg(feature = "sprites")]
    data.push(crate::sprites::DEFAULT_API_URL.into());
    data
}

#[test]
fn every_provider_tab_shows_only_messages_and_provider_names() {
    under_en_xa("Cloud devices en-XA leak", |cx, recorder| {
        let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
        let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
        recorder.take();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = SettingsWindow::new(weak, cx);
            // Unconfigured accounts read nothing from disk or the network.
            unconfigure(&mut view.config);
            view.section = Section::CloudDevices;
            view.open_cloud_devices(cx);
            window.focus(&view.focus, cx);
            view
        });
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(3000.)));
        let data = data();
        for &provider in CloudProvider::ALL {
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.cloud_tab = provider;
                    cx.notify();
                })
            });
            draw(cx);
            // Lines an earlier tab drew are not shaped again, so each tab is
            // checked by what it adds.
            assert_translated(
                &format!("Settings > Cloud devices > {provider:?}"),
                &recorder.take(),
                &data,
            );
        }
    });
}

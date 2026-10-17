use super::*;
use crate::{cloud::CloudProvider, menu::devices::rail as frame};
use gpui::{Entity, Modifiers, VisualTestContext, px, size};

fn open(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // Not configured from the environment either.
            #[cfg(feature = "coder")]
            {
                view.config.coder.url = None;
            }
            #[cfg(feature = "daytona")]
            {
                view.config.daytona.api_url = None;
            }
            view.open_add_device(window, cx);
        });
    });
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let target = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(target, Modifiers::default());
    draw(cx);
}

fn page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    cx.update(|_, cx| view.read(cx).menu.page)
}

fn configured_elsewhere() -> bool {
    std::env::var_os("HERDR_CODER_URL").is_some()
        || std::env::var_os("HERDR_DAYTONA_API_URL").is_some()
}

#[gpui::test]
fn every_provider_sits_beside_ssh_and_says_whether_it_is_set_up(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        // Nothing offers to create a device that could never connect. Windows
        // still lists its WSL distributions beside SSH.
        assert_eq!(cx.debug_bounds("add-device-rail").is_some(), cfg!(windows));
        return;
    }
    assert_eq!(page(&view, cx), Some(Page::AddDevice));
    assert!(cx.debug_bounds("add-device-source-host").is_some());
    for provider in CloudProvider::ALL {
        let id = match provider {
            #[cfg(feature = "azure")]
            CloudProvider::Azure => "add-device-source-azure",
            #[cfg(feature = "boat")]
            CloudProvider::Boat => "add-device-source-boat",
            #[cfg(feature = "coder")]
            CloudProvider::Coder => "add-device-source-coder",
            #[cfg(feature = "daytona")]
            CloudProvider::Daytona => "add-device-source-daytona",
            #[cfg(feature = "railway")]
            CloudProvider::Railway => "add-device-source-railway",
        };
        assert!(cx.debug_bounds(id).is_some(), "{provider:?} is listed");
    }
    if configured_elsewhere() {
        return;
    }
    #[cfg(feature = "coder")]
    {
        assert!(cx.debug_bounds("add-device-source-coder-ready").is_none());
        // Unconfigured, Coder's pane points to Settings.
        click(cx, "add-device-source-coder");
        assert_eq!(page(&view, cx), Some(Page::AddCloud(CloudProvider::Coder)));
        assert!(cx.debug_bounds("cloud-setup-settings").is_some());

        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.config.coder.url = Some("https://coder.example.com".into());
            })
        });
        draw(cx);
        assert!(cx.debug_bounds("add-device-source-coder-ready").is_some());
    }
}

#[gpui::test]
fn switching_sources_keeps_what_the_ssh_form_holds(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    cx.update(|_, cx| {
        let target = view.read(cx).menu.device_setup.as_ref().unwrap().fields[0].clone();
        target.update(cx, |field, cx| field.set_text_selected("me@build-box", cx));
    });
    let provider = CloudProvider::ALL[CloudProvider::ALL.len() - 1];
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_source(frame::Source::Cloud(provider), window, cx)
        })
    });
    draw(cx);
    assert_ne!(page(&view, cx), Some(Page::AddDevice));
    click(cx, "add-device-source-host");
    assert_eq!(page(&view, cx), Some(Page::AddDevice));
    cx.update(|window, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        assert_eq!(setup.fields[0].read(cx).text(), "me@build-box");
        assert!(setup.fields[0].read(cx).focus.is_focused(window));
    });
}

/// Providers whose pane is the Settings hand-off, with their rail entries.
fn settings_only() -> Vec<(CloudProvider, &'static str)> {
    CloudProvider::ALL
        .iter()
        .filter_map(|&provider| match provider {
            #[cfg(feature = "azure")]
            CloudProvider::Azure => Some((provider, "add-device-source-azure")),
            #[cfg(feature = "boat")]
            CloudProvider::Boat => Some((provider, "add-device-source-boat")),
            #[cfg(feature = "coder")]
            CloudProvider::Coder => None,
            #[cfg(feature = "daytona")]
            CloudProvider::Daytona => Some((provider, "add-device-source-daytona")),
            #[cfg(feature = "railway")]
            CloudProvider::Railway => Some((provider, "add-device-source-railway")),
        })
        .collect()
}

fn settings_opened(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) -> Option<CloudProvider> {
    cx.update(|_, cx| view.read(cx).menu.settings_opened)
}

#[gpui::test]
fn the_settings_button_opens_that_providers_settings(cx: &mut gpui::TestAppContext) {
    for (provider, entry) in settings_only() {
        let (view, cx) = open(cx);
        if crate::cloud::unavailable().is_some() {
            return;
        }
        click(cx, entry);
        assert_eq!(page(&view, cx), Some(Page::AddCloud(provider)));
        click(cx, "cloud-setup-settings");
        assert_eq!(settings_opened(&view, cx), Some(provider));
        assert_eq!(page(&view, cx), None, "the dialog closes for Settings");
    }
}

#[gpui::test]
fn enter_opens_that_providers_settings(cx: &mut gpui::TestAppContext) {
    for (provider, entry) in settings_only() {
        let (view, cx) = open(cx);
        if crate::cloud::unavailable().is_some() {
            return;
        }
        click(cx, entry);
        cx.simulate_keystrokes("enter");
        assert_eq!(settings_opened(&view, cx), Some(provider));
        assert_eq!(page(&view, cx), None);
    }
}

#[gpui::test]
fn escape_closes_a_settings_pane_without_opening_settings(cx: &mut gpui::TestAppContext) {
    for (_, entry) in settings_only() {
        let (view, cx) = open(cx);
        if crate::cloud::unavailable().is_some() {
            return;
        }
        click(cx, entry);
        cx.simulate_keystrokes("escape");
        assert_eq!(page(&view, cx), None);
        assert_eq!(settings_opened(&view, cx), None);
    }
}

#[gpui::test]
fn an_ssh_save_finishing_elsewhere_keeps_the_shown_source(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let provider = CloudProvider::ALL[CloudProvider::ALL.len() - 1];
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_source(frame::Source::Cloud(provider), window, cx);
            let shown = view.menu.page;
            view.menu.device_setup.as_mut().unwrap().step = Step::Saved;
            view.poll_device_setup(window, cx);
            assert_eq!(view.menu.page, shown, "the cloud form stays open");
            assert!(view.menu.device_setup.is_none(), "only the SSH form went");
            // SSH starts afresh on the next visit.
            view.open_source(frame::Source::Host, window, cx);
            assert_eq!(view.menu.page, Some(Page::AddDevice));
            assert!(matches!(
                view.menu.device_setup.as_ref().unwrap().step,
                Step::Form
            ));
        })
    });
}

#[gpui::test]
fn each_source_keeps_its_own_error(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let provider = CloudProvider::ALL[CloudProvider::ALL.len() - 1];
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_source(frame::Source::Cloud(provider), window, cx)
        })
    });
    // An SSH check fails while the cloud pane shows.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.set_device_error(Some("SSH to box: refused".into()))
        })
    });
    draw(cx);
    assert!(
        cx.debug_bounds("cloud-setup-error").is_none(),
        "not in the cloud pane"
    );
    click(cx, "add-device-source-host");
    cx.update(|_, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        assert_eq!(
            setup.error.as_deref(),
            Some("SSH to box: refused"),
            "kept for SSH"
        );
    });
}

#[gpui::test]
fn the_keyboard_reaches_every_source(cx: &mut gpui::TestAppContext) {
    // The app's own key bindings, so a global shortcut cannot take the keys.
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_add_device(window, cx)));
    draw(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let count = 1 + CloudProvider::ALL.len();
    let host = Some(Page::AddDevice);
    // Ctrl-Tab steps through every source from the SSH field and wraps.
    let mut seen = vec![page(&view, cx)];
    for _ in 1..count {
        cx.simulate_keystrokes("ctrl-tab");
        seen.push(page(&view, cx));
    }
    assert_eq!(seen.len(), count);
    for (index, shown) in seen.iter().enumerate() {
        assert!(!seen[..index].contains(shown), "{seen:?} visits each once");
    }
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(page(&view, cx), host, "wraps back to SSH");
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(page(&view, cx), seen[count - 1], "and backwards");

    // Plain Tab still moves between the SSH form's fields.
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(page(&view, cx), host);
    cx.simulate_keystrokes("tab");
    assert_eq!(page(&view, cx), host);
}

#[test]
fn the_picker_names_cloud_machines_only_when_offered() {
    assert_ne!(
        frame::add_device_offer(true),
        frame::add_device_offer(false)
    );
    assert!(frame::add_device_offer(true).contains("cloud"));
    assert!(!frame::add_device_offer(false).contains("cloud"));
}

#[gpui::test]
fn switching_sources_never_resizes_the_dialog(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let ssh = cx.debug_bounds("menu-panel").unwrap();
    for provider in CloudProvider::ALL {
        let id = match provider {
            #[cfg(feature = "azure")]
            CloudProvider::Azure => "add-device-source-azure",
            #[cfg(feature = "boat")]
            CloudProvider::Boat => "add-device-source-boat",
            #[cfg(feature = "coder")]
            CloudProvider::Coder => "add-device-source-coder",
            #[cfg(feature = "daytona")]
            CloudProvider::Daytona => "add-device-source-daytona",
            #[cfg(feature = "railway")]
            CloudProvider::Railway => "add-device-source-railway",
        };
        click(cx, id);
        assert_eq!(cx.debug_bounds("menu-panel").unwrap(), ssh, "{provider:?}");
    }
    click(cx, "add-device-source-host");
    assert_eq!(cx.debug_bounds("menu-panel").unwrap(), ssh);
    // The fixed height still leaves the dialog inside a short window.
    cx.simulate_resize(size(px(900.), px(400.)));
    draw(cx);
    assert!(cx.debug_bounds("menu-panel").unwrap().bottom() <= px(400.));
}

/// Windows offers its WSL distributions as a second source after SSH hosts,
/// and the dialog opens on the SSH form.
#[cfg(windows)]
#[gpui::test]
fn windows_lists_wsl_after_ssh_hosts(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    assert_eq!(page(&view, cx), Some(Page::AddDevice));
    let host = cx.debug_bounds("add-device-source-host").unwrap();
    let wsl = cx.debug_bounds("add-device-source-wsl").unwrap();
    assert!(host.bottom() <= wsl.top());
    assert_eq!(frame::Source::of(Page::AddWsl), Some(frame::Source::Wsl));
    assert!(frame::add_device_offer(false).contains("SSH host"));
    assert!(frame::add_device_offer(false).contains("WSL"));
}

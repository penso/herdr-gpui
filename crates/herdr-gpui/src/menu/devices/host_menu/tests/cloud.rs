//! A cloud device's header menu: rename, its provider's settings, its setup
//! script, and removal from this computer. Port forwards and server
//! keybindings go through an SSH profile, so a cloud device does not offer them.

use super::super::CLOUD_ACTIONS;
use super::*;
use crate::cloud::CloudProvider;

fn provider() -> CloudProvider {
    CloudProvider::ALL[0]
}

fn add_cloud(view: &mut crate::HerdrWindow) -> String {
    view.endpoints[0].collapsed = true;
    let id = format!("{}:w1", provider().key());
    view.endpoints.push(crate::endpoint::Endpoint::new(
        id.clone(),
        "Build box".into(),
        herdr_client::ConnectTarget::Cloud {
            provider: provider(),
            account: "https://cloud.example.com".into(),
            id: "w1".into(),
            machine: "herdr-tidy-lark-aa".into(),
            session: "default".into(),
        },
        true,
    ));
    id
}

#[gpui::test]
fn right_clicking_a_cloud_device_offers_rename_settings_setup_and_removal(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        add_cloud(&mut view);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let header = match provider() {
        #[cfg(feature = "azure")]
        CloudProvider::Azure => "host-azure:w1",
        #[cfg(feature = "boat")]
        CloudProvider::Boat => "host-boat:w1",
        #[cfg(feature = "coder")]
        CloudProvider::Coder => "host-coder:w1",
        #[cfg(feature = "daytona")]
        CloudProvider::Daytona => "host-daytona:w1",
        #[cfg(feature = "railway")]
        CloudProvider::Railway => "host-railway:w1",
    };
    let header = cx.debug_bounds(header).unwrap();
    cx.simulate_mouse_down(header.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(header.center(), MouseButton::Right, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::Host));
        assert_eq!(
            host(view).saved,
            Saved::Cloud {
                provider: provider(),
                id: "w1".into()
            }
        );
        // Named by the machine, which an empty new name falls back to.
        assert_eq!(host(view).target, "herdr-tidy-lark-aa");
        assert_eq!(host(view).actions(), CLOUD_ACTIONS);
        assert_eq!(host(view).ssh_target(), None);
        assert!(!view.host_menu_lists_forwards());
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let rows = [
        "host-menu-0",
        "host-menu-1",
        "host-menu-2",
        "host-menu-3",
        "host-menu-4",
        "host-menu-5",
    ];
    let (shown, past) = rows.split_at(CLOUD_ACTIONS.len());
    for row in shown {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    assert!(cx.debug_bounds(past[0]).is_none());

    // Removal is last, and says the machine itself stays at the provider.
    cx.simulate_keystrokes("up enter");
    assert!(view.read_with(cx, |view, _| view.menu.page == Some(Page::RemoveDevice)));
    view.read_with(cx, |view, _| {
        let note = super::super::removal_note(host(view));
        assert!(note.contains("Build box (herdr-tidy-lark-aa)"), "{note}");
        assert!(note.contains("itself is left as it is"), "{note}");
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("remove-device").is_some());
    assert!(cx.debug_bounds("remove-device-github").is_none());
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}

#[gpui::test]
fn a_cloud_device_s_menu_renames_in_place_and_opens_its_provider_settings(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let id = add_cloud(view);
            view.open_host_menu(&id, point(px(10.), px(10.)), window, cx);
            view.activate_host_menu(Action::Rename, window, cx);
            assert_eq!(view.menu.page, Some(Page::RenameDevice));
            let input = host(view).input.clone().unwrap();
            assert_eq!(input.read(cx).text(), "Build box");

            view.dismiss_menu(window, cx);
            view.open_host_menu(&id, point(px(10.), px(10.)), window, cx);
            view.activate_host_menu(Action::ProviderSettings, window, cx);
            assert_eq!(view.menu.settings_opened, Some(provider()));
            assert!(view.menu.page.is_none());
        });
    });
}

#[test]
fn cloud_renames_fall_back_to_the_machine_name() {
    assert_eq!(
        crate::menu::devices::setup::device_label("  ", "herdr-tidy-lark-aa").unwrap(),
        "herdr-tidy-lark-aa"
    );
}

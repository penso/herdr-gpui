use super::*;
use crate::cloud::CloudProvider;
use gpui::{Entity, Modifiers, VisualTestContext, px, size};

fn open(cx: &mut gpui::TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_add_device(window, cx)));
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

fn page(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    cx.update(|_, cx| view.read(cx).menu.page)
}

/// The Add provider tile for `provider`.
fn tile_id(provider: CloudProvider) -> &'static str {
    format!("add-provider-{}", provider.key()).leak()
}

/// Save a device made from `provider`, as a finished create does.
fn add_device(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, provider: CloudProvider) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = format!("{}:herdr-made", provider.key());
            view.endpoints.push(crate::endpoint::Endpoint::new(
                id,
                "herdr-made".into(),
                sheprd_client::ConnectTarget::Cloud {
                    provider,
                    account: "account".into(),
                    id: "herdr-made".into(),
                    machine: "herdr-made".into(),
                    session: "default".into(),
                },
                true,
            ));
            cx.notify();
        })
    });
    draw(cx);
}

#[gpui::test]
fn a_tile_opens_its_providers_settings(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let Some(&provider) = cx.update(|_, cx| view.read(cx).cloud_unlisted()).first() else {
        return;
    };
    click(cx, "add-device-source-catalog");
    click(cx, tile_id(provider));
    cx.update(|_, cx| {
        let menu = &view.read(cx).menu;
        assert_eq!(menu.settings_opened, Some(provider));
        assert!(!menu.cloud_settings_opened);
    });
    assert_eq!(page(&view, cx), None, "the dialog closes for Settings");
}

#[gpui::test]
fn open_settings_and_enter_open_cloud_devices(cx: &mut gpui::TestAppContext) {
    for open_with in ["button", "enter"] {
        let (view, cx) = open(cx);
        if crate::cloud::unavailable().is_some() {
            return;
        }
        click(cx, "add-device-source-catalog");
        match open_with {
            "button" => click(cx, "add-provider-settings"),
            _ => cx.simulate_keystrokes("enter"),
        }
        cx.update(|_, cx| {
            let menu = &view.read(cx).menu;
            assert!(menu.cloud_settings_opened, "{open_with}");
            assert_eq!(menu.settings_opened, None, "no provider's tab");
        });
        assert_eq!(page(&view, cx), None);
    }
}

#[gpui::test]
fn escape_closes_add_provider_without_settings(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    click(cx, "add-device-source-catalog");
    cx.simulate_keystrokes("escape");
    assert_eq!(page(&view, cx), None);
    cx.update(|_, cx| assert!(!view.read(cx).menu.cloud_settings_opened));
}

#[gpui::test]
fn a_provider_with_a_device_moves_into_the_rail(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let Some(&provider) = cx.update(|_, cx| view.read(cx).cloud_unlisted()).first() else {
        return;
    };
    add_device(&view, cx, provider);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.cloud_listed().contains(&provider));
        assert!(!view.cloud_unlisted().contains(&provider));
    });
    let ready: &'static str = format!("{}-ready", rail::rail_id(provider)).leak();
    assert!(cx.debug_bounds(ready).is_some());
    click(cx, "add-device-source-catalog");
    assert!(cx.debug_bounds(tile_id(provider)).is_none());
}

#[gpui::test]
fn add_provider_leaves_the_rail_once_every_provider_is_in_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    for &provider in CloudProvider::ALL {
        add_device(&view, cx, provider);
    }
    assert!(cx.debug_bounds("add-device-source-catalog").is_none());
    for &provider in CloudProvider::ALL {
        assert!(cx.debug_bounds(rail::rail_id(provider)).is_some());
    }
}

#[gpui::test]
fn a_pane_the_rail_omits_marks_add_provider(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let Some(&provider) = cx.update(|_, cx| view.read(cx).cloud_unlisted()).first() else {
        return;
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu.page = Some(Page::AddCloud(provider));
            cx.notify();
        })
    });
    draw(cx);
    assert_eq!(
        crate::menu::devices::rail::Source::of(Page::AddProvider),
        Some(crate::menu::devices::rail::Source::Catalog)
    );
    assert!(cx.debug_bounds("add-device-source-catalog").is_some());
}

#[gpui::test]
fn tiles_stack_in_one_column_in_a_narrow_window(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx);
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let unlisted = cx.update(|_, cx| view.read(cx).cloud_unlisted());
    let [first, second, ..] = unlisted[..] else {
        return;
    };
    click(cx, "add-device-source-catalog");
    let tile = |cx: &mut VisualTestContext, provider: CloudProvider| {
        cx.debug_bounds(tile_id(provider)).unwrap()
    };
    let (a, b) = (tile(cx, first), tile(cx, second));
    assert_eq!(a.top(), b.top(), "side by side");
    assert!(a.right() <= b.left());
    cx.simulate_resize(size(px(420.), px(700.)));
    draw(cx);
    let (a, b) = (tile(cx, first), tile(cx, second));
    assert!(a.bottom() <= b.top(), "stacked");
}

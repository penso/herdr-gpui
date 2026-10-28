use super::*;
use crate::{config::RemoteConfig, sidebar::layout_tests::fixture_window};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};
use herdr_remote_control::control::{self, DeviceId, Request, Response, Status};
use std::path::Path;

fn status(connected: usize) -> Status {
    Status {
        boot_id: String::new(),
        address_details: Default::default(),
        version: "dev".into(),
        port: 7443,
        addresses: vec!["https://192.168.1.20:7443".into()],
        fingerprint: "5F:2A".into(),
        herdr: true,
        chats: 0,
        devices: 2,
        connected,
    }
}

fn phone(id: &str, name: &str, connected: bool) -> Device {
    Device {
        id: DeviceId(id.into()),
        name: name.into(),
        created_ms: 0,
        last_seen_ms: Some(0),
        push: true,
        connected,
    }
}

fn unreachable(_: &Path, _: &Request) -> Result<Response, control::Error> {
    Err(control::Error::NoAnswer)
}

/// A window whose daemon runs with `devices` paired, `connected` of them live.
fn window(
    cx: &mut TestAppContext,
    devices: Vec<Device>,
    connected: usize,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let config = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    cx.update(|_, cx| {
        Supervisor::fixture(cx, config.clone(), unreachable);
        Supervisor::set_health(cx, Health::Running(status(connected)), Some(devices));
    });
    view.update(cx, |view, _| view.config.remote = config);
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn the_title_bar_offers_pairing_until_a_phone_is_paired(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, Vec::new(), 0);
    assert!(cx.debug_bounds("titlebar-remote").is_some());
    let remote = cx.debug_bounds("titlebar-remote").unwrap();
    let account = cx.debug_bounds("titlebar-account-slot").unwrap();
    assert!(remote.right() <= account.left(), "{remote:?} {account:?}");
    cx.simulate_click(remote.center(), Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteControl));
    });
}

#[gpui::test]
fn the_title_bar_lists_paired_phones_and_disconnects_one(cx: &mut TestAppContext) {
    let devices = vec![phone("d1", "iPhone", true), phone("d2", "iPad", false)];
    let (view, cx) = window(cx, devices, 1);
    assert!(cx.debug_bounds("titlebar-remote-live").is_some());
    let remote = cx.debug_bounds("titlebar-remote").unwrap();
    cx.simulate_click(remote.center(), Modifiers::none());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteDevices));
    });
    let menu = cx.debug_bounds("remote-menu").unwrap();
    assert!(menu.top() >= remote.bottom(), "{menu:?} {remote:?}");
    assert!(cx.debug_bounds("remote-menu-device-1").is_some());
    assert!(cx.debug_bounds("remote-menu-pair").is_some());
    let disconnect = cx.debug_bounds("remote-menu-device-1-disconnect").unwrap();
    cx.simulate_click(disconnect.center(), Modifiers::none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RevokeDevice));
        let asked = view.menu.remote_revoke.as_ref().unwrap();
        assert_eq!(asked.device.id, DeviceId("d2".into()));
    });
}

#[gpui::test]
fn no_phone_shows_while_remote_control_is_off(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, vec![phone("d1", "iPhone", true)], 1);
    view.update(cx, |view, cx| {
        view.config.remote.enabled = false;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("titlebar-remote").is_none());
}

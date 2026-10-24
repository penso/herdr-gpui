#![allow(clippy::unwrap_used)]
use super::*;
use crate::{config::RemoteConfig, remote::revoke::Revoke, sidebar::layout_tests::fixture_window};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};
use herdr_remote_control::control::{
    self, Device, DeviceId, Offer, PairingId, PairingState, Response,
};
use std::{
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

fn on() -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    }
}

fn offer() -> Offer {
    Offer {
        id: PairingId("p1".into()),
        url: "https://192.168.1.20:7443/pair#t=TOKEN&fp=5F:2A".into(),
        fingerprint: "5F:2A:C1:9E".into(),
        expires_in_ms: 60_000,
        addresses: vec![
            "https://192.168.1.20:7443".into(),
            "https://100.64.0.3:7443".into(),
        ],
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// A window whose supervisor asks `send`, with remote control on or off.
fn window(
    cx: &mut TestAppContext,
    enabled: bool,
    send: super::super::supervisor::Send,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let config = RemoteConfig { enabled, ..on() };
    cx.update(|_, cx| Supervisor::fixture(cx, config.clone(), send));
    view.update(cx, |view, _| view.config.remote = config);
    (view, cx)
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    let opened =
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_remote_pairing(window, cx)));
    assert!(opened);
}

fn phase(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Phase {
    view.read_with(cx, |view, _| {
        view.menu
            .remote_pairing
            .as_ref()
            .map(|dialog| dialog.pairing.phase().clone())
            .unwrap()
    })
}

/// Lets the dialog's loop poll again.
fn wait(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
}

fn unreachable(_: &Path, _: &Request) -> Result<Response, control::Error> {
    Err(control::Error::NoAnswer)
}

#[gpui::test]
fn off_explains_how_to_turn_it_on(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, false, unreachable);
    open(&view, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteControl));
    });
    assert_eq!(phase(&view, cx), Phase::Off);
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-settings").is_some());
    assert!(cx.debug_bounds("remote-pair-qr").is_none());
}

#[gpui::test]
fn an_unreachable_daemon_offers_to_try_again(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, unreachable);
    open(&view, cx);
    cx.run_until_parked();
    assert!(matches!(phase(&view, cx), Phase::Failed { .. }));
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-retry").is_some());
}

static PAIR_LOG: Mutex<Vec<Request>> = Mutex::new(Vec::new());
static CLAIMED: AtomicBool = AtomicBool::new(false);
static APPROVED: AtomicBool = AtomicBool::new(false);

fn pairing_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    PAIR_LOG.lock().unwrap().push(request.clone());
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } if APPROVED.load(Ordering::SeqCst) => {
            Response::Pairing(PairingState::Approved {
                device: DeviceId("d1".into()),
            })
        }
        Request::Pairing { .. } if CLAIMED.load(Ordering::SeqCst) => {
            Response::Pairing(PairingState::Claimed {
                device_name: "Pixel\u{202e}".into(),
                code: "428913".into(),
            })
        }
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        Request::Approve { allow: true, .. } => {
            APPROVED.store(true, Ordering::SeqCst);
            Response::Pairing(PairingState::Approved {
                device: DeviceId("d1".into()),
            })
        }
        _ => Response::Done,
    })
}

#[gpui::test]
fn a_phone_scans_claims_and_is_allowed(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, pairing_daemon);
    open(&view, cx);
    cx.run_until_parked();
    assert!(matches!(phase(&view, cx), Phase::Showing { .. }));
    draw(cx);
    let qr = cx.debug_bounds("remote-pair-qr").unwrap();
    assert_eq!(qr.size.width, qr.size.height);
    assert!(cx.debug_bounds("remote-pair-countdown").is_some());
    assert!(cx.debug_bounds("remote-pair-fingerprint").is_some());
    CLAIMED.store(true, Ordering::SeqCst);
    wait(cx);
    let Phase::Claimed { code, .. } = phase(&view, cx) else {
        panic!("{:?}", phase(&view, cx));
    };
    assert_eq!(spaced(&code), "428 913");
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-code").is_some());
    assert!(cx.debug_bounds("remote-pair-qr").is_none());
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    wait(cx);
    assert!(matches!(phase(&view, cx), Phase::Approved { .. }));
    // The device name is drawn without its direction override.
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-done").is_some());
    let log = PAIR_LOG.lock().unwrap().clone();
    assert!(log.contains(&Request::Approve {
        id: PairingId("p1".into()),
        allow: true,
        code: "428913".into(),
    }));
    // A finished pairing is not cancelled on close.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    let log = PAIR_LOG.lock().unwrap().clone();
    assert!(
        !log.iter()
            .any(|request| matches!(request, Request::CancelPairing { .. }))
    );
}

static CANCEL_LOG: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn cancel_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    CANCEL_LOG.lock().unwrap().push(request.clone());
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        _ => Response::Done,
    })
}

#[gpui::test]
fn closing_the_dialog_cancels_its_pairing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, cancel_daemon);
    open(&view, cx);
    cx.run_until_parked();
    assert!(matches!(phase(&view, cx), Phase::Showing { .. }));
    draw(cx);
    let close = cx.debug_bounds("remote-pair-x").unwrap().center();
    cx.simulate_click(close, Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(view.menu.remote_pairing.is_none());
    });
    let log = CANCEL_LOG.lock().unwrap().clone();
    assert_eq!(
        log.last(),
        Some(&Request::CancelPairing {
            id: PairingId("p1".into())
        })
    );
}

#[gpui::test]
fn another_dialog_holds_the_window(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, false, unreachable);
    open(&view, cx);
    let again =
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_remote_pairing(window, cx)));
    assert!(!again);
}

static REVOKED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn revoking_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    REVOKED.lock().unwrap().push(request.clone());
    Ok(Response::Done)
}

fn pixel() -> Device {
    Device {
        id: DeviceId("d7".into()),
        name: "Pixel".into(),
        created_ms: 0,
        last_seen_ms: None,
        push: true,
        connected: true,
    }
}

#[gpui::test]
fn revoking_asks_first_and_sends_only_on_confirm(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, revoking_daemon);
    let asked =
        cx.update(|window, cx| view.update(cx, |view, cx| view.ask_to_revoke(pixel(), window, cx)));
    assert!(asked);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RevokeDevice));
        assert!(matches!(&view.menu.remote_revoke, Some(Revoke { device }) if device.id.0 == "d7"));
    });
    draw(cx);
    let cancel = cx.debug_bounds("remote-revoke-cancel").unwrap().center();
    cx.simulate_click(cancel, Modifiers::default());
    cx.run_until_parked();
    assert!(REVOKED.lock().unwrap().is_empty(), "cancel sends nothing");
    cx.update(|window, cx| view.update(cx, |view, cx| view.ask_to_revoke(pixel(), window, cx)));
    draw(cx);
    let submit = cx.debug_bounds("remote-revoke-submit").unwrap().center();
    cx.simulate_click(submit, Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    assert_eq!(
        *REVOKED.lock().unwrap(),
        [Request::Revoke {
            device: DeviceId("d7".into())
        }]
    );
}

#[test]
fn the_ring_runs_clockwise_from_twelve_for_its_share() {
    let full = arc((10., 10.), 5., 1., 48);
    assert_eq!(full.len(), 49);
    let (x, y) = full[0];
    assert!(
        (x - 10.).abs() < 1e-4 && (y - 5.).abs() < 1e-4,
        "starts at twelve"
    );
    let (x, y) = full[12];
    assert!(
        (x - 15.).abs() < 1e-4 && (y - 10.).abs() < 1e-4,
        "a quarter is at three"
    );
    let last = full[48];
    assert!(
        (last.0 - 10.).abs() < 1e-3 && (last.1 - 5.).abs() < 1e-3,
        "closes"
    );
    let quarter = arc((0., 0.), 1., 0.25, 48);
    assert_eq!(quarter.len(), 13);
    let end = quarter[12];
    assert!((end.0 - 1.).abs() < 1e-4 && end.1.abs() < 1e-4);
    // A sliver still draws a segment; nothing left draws nothing.
    assert_eq!(arc((0., 0.), 1., 0.001, 48).len(), 2);
    for empty in [0., -1., f32::NAN] {
        assert!(arc((0., 0.), 1., empty, 48).is_empty());
    }
    assert!(arc((0., 0.), 0., 1., 48).is_empty());
    assert_eq!(arc((0., 0.), 1., 7., 48).len(), 49, "clamped to a turn");
}

#[test]
fn addresses_drop_their_scheme_and_unsafe_text() {
    assert_eq!(
        addresses(&[
            "https://192.168.1.20:7443".into(),
            "https://100.64.0.3:7443\u{202e}".into(),
        ]),
        "192.168.1.20:7443 \u{b7} 100.64.0.3:7443"
    );
    assert_eq!(name("  "), "A device");
    assert_eq!(name("Pixel\u{202e}"), "Pixel");
}

//! The approval step lists the other machines, ticked, and the approval
//! names the ones still ticked; a phone's later request for one is asked
//! about in its own dialog.

use super::*;
use crate::remote::links::Known;
use herdr_client::ConnectTarget;
use herdr_remote_control::control::hosts::{Host, HostId, HostOutcome, HostPairing};

static LOG: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn claimed_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    LOG.lock().unwrap().push(request.clone());
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => Response::Pairing(PairingState::Claimed {
            device_name: "iPhone".into(),
            code: "428913".into(),
            app: None,
        }),
        Request::Approve { allow: true, .. } => Response::Pairing(PairingState::Approved {
            device: DeviceId("d1".into()),
        }),
        _ => Response::Done,
    })
}

fn known(id: &str, name: &str) -> Known {
    Known {
        host: Host {
            id: HostId(id.into()),
            name: name.into(),
        },
        target: ConnectTarget::Ssh {
            target: name.into(),
            session: "default".into(),
        },
    }
}

#[gpui::test]
fn allowing_a_phone_allows_the_machines_left_ticked(cx: &mut TestAppContext) {
    LOG.lock().unwrap().clear();
    let (view, cx) = window(cx, true, claimed_daemon);
    cx.update(|_, cx| {
        Supervisor::set_hosts(
            cx,
            vec![known("ssh:1", "devbox"), known("ssh:2", "gpu box")],
        );
    });
    open(&view, cx);
    // The window's own endpoints would replace the list on its next poll.
    view.update(cx, |view, _| {
        view.menu.remote_pairing.as_mut().unwrap().hosts = super::super::hosts::choices(vec![
            known("ssh:1", "devbox").host,
            known("ssh:2", "gpu box").host,
        ]);
    });
    wait(cx);
    assert!(matches!(phase(&view, cx), Phase::Claimed { .. }));
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-hosts").is_some());
    let gpu = cx.debug_bounds("remote-pair-host-1").unwrap().center();
    cx.simulate_click(gpu, Modifiers::default());
    draw(cx);
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    let log = LOG.lock().unwrap().clone();
    assert!(log.contains(&Request::Approve {
        id: PairingId("p1".into()),
        allow: true,
        code: "428913".into(),
        hosts: vec![HostId("ssh:1".into())],
    }));
}

#[gpui::test]
fn a_later_request_is_asked_about_and_closing_declines(cx: &mut TestAppContext) {
    LOG.lock().unwrap().clear();
    let (view, cx) = window(cx, true, claimed_daemon);
    let job = crate::remote::links::Job {
        pairing: HostPairing {
            device: DeviceId("d1".into()),
            host: HostId("ssh:9".into()),
            name: "iPhone".into(),
            public_key: "KEY".into(),
            confirm: true,
            replace: None,
        },
        target: None,
        via: "MacBook Pro".into(),
        apns: None,
    };
    cx.update(|_, cx| Supervisor::queue_host_ask_for_tests(cx, job));
    // Only the window in front asks.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |view, cx| view.sync_remote_hosts(window, cx)));
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteHostAsk));
    });
    draw(cx);
    assert!(cx.debug_bounds("remote-host-ask-allow").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    assert!(LOG.lock().unwrap().iter().any(|request| matches!(
        request,
        Request::HostPaired { host, outcome: HostOutcome::Denied, .. } if host.0 == "ssh:9"
    )));
}

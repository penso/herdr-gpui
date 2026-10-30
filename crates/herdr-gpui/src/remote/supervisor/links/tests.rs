//! The supervisor's host work: what each poll sends, and which requests
//! run at once or wait for the user.

use super::*;
use gpui::TestAppContext;

mod report;
use herdr_remote_control::control::hosts::HostPairing;
use std::sync::Mutex;

fn known(id: &str) -> Known {
    Known {
        host: Host {
            id: HostId(id.into()),
            name: id.into(),
        },
        target: ConnectTarget::Ssh {
            target: format!("{id}.example"),
            session: "default".into(),
        },
    }
}

fn pairing(host: &str, confirm: bool) -> HostPairing {
    HostPairing {
        device: DeviceId("d1".into()),
        host: HostId(host.into()),
        name: "iPhone".into(),
        public_key: "KEY".into(),
        confirm,
        replace: None,
    }
}

#[test]
fn the_host_list_is_sent_when_it_changes_or_the_daemon_restarts() {
    let mut links = Links {
        known: vec![known("ssh:a")],
        ..Links::default()
    };
    let first = links.outgoing();
    assert_eq!(first.hosts.as_ref().map(Vec::len), Some(1));
    assert!(first.revokes);
    links.sent = first.hosts.map(|hosts| (hosts, Instant::now()));
    links.revokes_checked = Some(Instant::now());
    let again = links.outgoing();
    assert!(again.hosts.is_none());
    assert!(!again.revokes);
    links.known.push(known("ssh:b"));
    assert_eq!(links.outgoing().hosts.as_ref().map(Vec::len), Some(2));
    links.known.pop();
    links.lost();
    let after = links.outgoing();
    assert!(after.hosts.is_some());
    assert!(after.revokes);
}

static SENT: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    SENT.lock().unwrap().push(request.clone());
    Ok(Response::Done)
}

fn on() -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    }
}

#[gpui::test]
fn a_host_not_allowed_waits_for_the_user_and_a_gone_host_fails(cx: &mut TestAppContext) {
    SENT.lock().unwrap().clear();
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), daemon);
        Supervisor::set_hosts(cx, vec![known("ssh:lab")]);
        Supervisor::take_incoming(
            cx,
            Incoming {
                sent: None,
                pairings: vec![pairing("ssh:lab", true), pairing("ssh:gone", false)],
                revokes: None,
            },
        );
    });
    cx.run_until_parked();
    // The host this app no longer connects to fails at once.
    let sent = SENT.lock().unwrap().clone();
    assert!(sent.iter().any(|request| matches!(
        request,
        Request::HostPaired { host, outcome: HostOutcome::Failed { reason }, .. }
            if host.0 == "ssh:gone" && reason.contains("no longer connects")
    )));
    // The other waits for a window to ask, once.
    let job = cx.update(Supervisor::take_host_ask).unwrap();
    assert_eq!(job.pairing.host.0, "ssh:lab");
    assert!(job.target.is_some());
    assert!(cx.update(Supervisor::take_host_ask).is_none());
    // The user says no: the daemon hears it.
    cx.update(|cx| Supervisor::answer_host(cx, job, false));
    cx.run_until_parked();
    assert!(SENT.lock().unwrap().iter().any(|request| matches!(
        request,
        Request::HostPaired { host, outcome: HostOutcome::Denied, .. } if host.0 == "ssh:lab"
    )));
}

#[gpui::test]
fn a_request_already_running_is_not_started_twice(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), daemon);
        let incoming = || Incoming {
            sent: None,
            pairings: vec![pairing("ssh:lab", true)],
            revokes: None,
        };
        Supervisor::take_incoming(cx, incoming());
        Supervisor::take_incoming(cx, incoming());
        assert_eq!(cx.global::<Supervisor>().links.asks.len(), 1);
    });
}

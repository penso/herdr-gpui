use super::super::ports::{forward_info, list, port_info, unforward};
use super::*;
use crate::{
    control::protocol::hosts::Ports,
    listening_ports::{Bind, Origin, Owner, Port},
    port_forward::{PortForwards, State},
};
use std::net::{IpAddr, Ipv4Addr};

fn port(number: u16, bind: Bind) -> Port {
    Port {
        number,
        bind,
        process: "node".into(),
    }
}

fn remote() -> Origin {
    Origin::Remote {
        target: "me@devbox".into(),
        name: Some("devbox".into()),
    }
}

#[test]
fn a_port_says_where_this_machine_reaches_it() {
    let local = port_info(&port(3000, Bind::Any), &Origin::Local, &[]);
    assert_eq!(local.port, 3000);
    assert_eq!(local.bind, "*:3000");
    assert_eq!(local.process.as_deref(), Some("node"));
    assert_eq!(local.local_url.as_deref(), Some("http://localhost:3000/"));
    let lan = port_info(
        &port(8080, Bind::Address(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)))),
        &remote(),
        &[],
    );
    assert_eq!(lan.local_url.as_deref(), Some("http://10.0.0.2:8080/"));
    // A remote loopback port is reachable only through a forward.
    let hidden = port_info(&port(5173, Bind::Loopback), &remote(), &[]);
    assert_eq!(hidden.local_url, None);
    let mut forwards = PortForwards::default();
    forwards.fixture("me@devbox", 5173, State::Listening { local_port: 40_001 });
    let target = "me@devbox".into();
    let listed: Vec<_> = forwards.for_host(&target).collect();
    let forwarded = port_info(&port(5173, Bind::Loopback), &remote(), &listed);
    assert_eq!(
        forwarded.local_url.as_deref(),
        Some("http://127.0.0.1:40001/")
    );
}

#[test]
fn a_forward_reports_its_state_and_why_it_ended() {
    let mut forwards = PortForwards::default();
    forwards.fixture("me@devbox", 22, State::Starting);
    forwards.fixture("me@devbox", 80, State::Listening { local_port: 40_080 });
    forwards.fixture("me@devbox", 443, State::Ended("ssh exited".into()));
    let infos: Vec<_> = forwards
        .for_host(&"me@devbox".into())
        .map(|forward| forward_info("me@devbox", forward))
        .collect();
    let summary: Vec<_> = infos
        .iter()
        .map(|info| {
            (
                info.port,
                info.state.as_str(),
                info.local_url.as_deref(),
                info.reason.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (22, "starting", None, None),
            (80, "listening", Some("http://127.0.0.1:40080/"), None),
            (443, "ended", None, Some("ssh exited")),
        ]
    );
}

#[gpui::test]
fn ports_list_the_workspaces_listeners_or_say_why_not(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.listening_ports.seed(
                crate::usage::Host::Local,
                [(
                    Owner {
                        workspace: "w3".into(),
                        socket: String::new(),
                    },
                    vec![port(3000, Bind::Any)],
                )]
                .into(),
            );
            let Response::Host {
                reply: Reply::Ports(ports),
            } = list(view, &caller(Some("w3")))
            else {
                panic!("expected ports");
            };
            let Ports {
                host,
                workspace,
                listening,
                forwards,
                note: None,
            } = *ports
            else {
                panic!("expected no note");
            };
            assert_eq!((host.as_str(), workspace.as_str()), ("Local", "w3"));
            assert_eq!(listening.len(), 1);
            assert_eq!(listening[0].port, 3000);
            assert!(forwards.is_empty());
            view.config.show_listening_ports = false;
            let Response::Host {
                reply: Reply::Ports(ports),
            } = list(view, &caller(Some("w0")))
            else {
                panic!("expected ports");
            };
            let note = ports.note.unwrap_or_default();
            assert!(note.contains("show_listening_ports"), "{note}");
        })
    });
}

#[gpui::test]
fn local_ports_need_no_forward_and_a_remote_forward_stops(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let refused = super::super::ports::forward(view, &caller(None), 3000, cx);
            assert_eq!(error_code(&refused), Some(ErrorCode::InvalidRequest));
            view.endpoints.push(crate::endpoint::Endpoint::new(
                "ssh:devbox".into(),
                "Devbox".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "me@devbox".into(),
                    session: "default".into(),
                },
                true,
            ));
            // The window shows the remote host, with the fixture's workspaces.
            view.selected_endpoint = 1;
            view.port_forwards
                .fixture("me@devbox", 5173, State::Listening { local_port: 40_002 });
            let Response::Host {
                reply: Reply::Ports(ports),
            } = list(view, &caller(None))
            else {
                panic!("expected ports");
            };
            let forwards = ports.forwards;
            assert_eq!(forwards.len(), 1);
            assert_eq!(forwards[0].host, "me@devbox");
            let stopped = unforward(view, &caller(None), 5173, cx);
            assert!(matches!(stopped, Response::Done { .. }), "{stopped:?}");
            assert_eq!(view.port_forwards.for_host(&"me@devbox".into()).count(), 0);
            let again = unforward(view, &caller(None), 5173, cx);
            assert_eq!(error_code(&again), Some(ErrorCode::NotFound));
        })
    });
}

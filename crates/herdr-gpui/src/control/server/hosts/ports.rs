//! `ports`: the caller's workspace's listening ports, as the status bar
//! lists them, and this window's forwards of a saved SSH host's ports.
use super::{Place, Watches, socket, window_for};
use crate::{
    HerdrWindow,
    browser::{PageOpened, WebUrl},
    control::protocol::{
        Caller, ErrorCode, Response,
        hosts::{FORWARD_WAIT_SECONDS, ForwardInfo, PortInfo, Ports, Reply},
    },
    listening_ports::{Daemon, Link, Origin, Port},
    port_forward::{Forward, State},
    usage::Host,
};
use gpui::{App, Context, Window};
use herdr_client::SshTarget;
use std::{num::NonZeroU16, time::Duration};

pub(super) fn port_info(port: &Port, origin: &Origin, forwards: &[&Forward]) -> PortInfo {
    let local_url = match port.link(origin) {
        Some(Link::Page(url)) => Some(url.as_str().to_owned()),
        Some(Link::Tunnel(_)) | None => forwards
            .iter()
            .find(|forward| forward.remote_port().get() == port.number)
            .and_then(|forward| forward.url())
            .map(|url| url.as_str().to_owned()),
    };
    PortInfo {
        port: port.number,
        bind: port.address(),
        process: Some(port.process.clone()).filter(|process| !process.is_empty()),
        local_url,
    }
}

pub(super) fn forward_info(host: &str, forward: &Forward) -> ForwardInfo {
    let (state, reason) = match forward.state() {
        State::Starting => ("starting", None),
        State::Listening { .. } => ("listening", None),
        State::Ended(reason) => ("ended", Some(reason.clone())),
    };
    ForwardInfo {
        host: host.to_owned(),
        port: forward.remote_port().get(),
        state: state.to_owned(),
        local_url: forward.url().map(|url| url.as_str().to_owned()),
        reason,
    }
}

/// The saved SSH target of the place's host, which forwards are keyed by.
fn ssh_target<'a>(view: &'a HerdrWindow, place: &Place) -> Option<&'a SshTarget> {
    view.endpoints
        .get(place.index)?
        .saved_ssh()
        .map(|(target, _)| target)
}

/// Whether the place's host is this machine, or a WSL distribution whose
/// ports this machine reaches as its own.
fn local(view: &HerdrWindow, place: &Place) -> bool {
    view.endpoints
        .get(place.index)
        .and_then(|endpoint| Host::of(&endpoint.connection.target))
        .is_some_and(|host| matches!(host, Host::Local | Host::Wsl(_)))
}

fn forwards<'a>(view: &'a HerdrWindow, place: &Place) -> Vec<&'a Forward> {
    ssh_target(view, place)
        .map(|target| view.port_forwards.for_host(target).collect())
        .unwrap_or_default()
}

pub(super) fn list(view: &HerdrWindow, caller: &Caller) -> Response {
    let place = match Place::of(view, caller) {
        Ok(place) => place,
        Err(response) => return response,
    };
    let forwards = forwards(view, &place);
    let daemon = view
        .endpoints
        .get(place.index)
        .and_then(|endpoint| Daemon::of(&endpoint.connection.target));
    let listening: Vec<PortInfo> = daemon
        .as_ref()
        .and_then(|daemon| view.listening_ports.get(daemon, &place.workspace))
        .map(|listed| {
            listed
                .ports
                .iter()
                .map(|port| port_info(port, listed.origin, &forwards))
                .collect()
        })
        .unwrap_or_default();
    let note = if !view.config.show_listening_ports {
        Some("Herdr UI does not scan listening ports: show_listening_ports is off".to_owned())
    } else if daemon.is_none() {
        Some(format!("Herdr UI cannot scan ports on {}", place.label))
    } else {
        None
    };
    let host = ssh_target(view, &place)
        .map(|target| target.target.as_str())
        .unwrap_or(&place.label)
        .to_owned();
    Response::Host {
        reply: Reply::Ports(Box::new(Ports {
            listening,
            forwards: forwards
                .iter()
                .map(|forward| forward_info(&host, forward))
                .collect(),
            host: place.label,
            workspace: place.workspace,
            note,
        })),
    }
}

/// The SSH target to forward `place`'s ports from, or why there is none.
fn forwarding_target<'a>(view: &'a HerdrWindow, place: &Place) -> Result<&'a SshTarget, Response> {
    if let Some(target) = ssh_target(view, place) {
        return Ok(target);
    }
    Err(if local(view, place) {
        Response::error(
            ErrorCode::InvalidRequest,
            "Local ports need no forwarding; open them at localhost",
        )
    } else {
        Response::error(
            ErrorCode::Unsupported,
            format!(
                "Only a saved SSH host's ports can be forwarded, not {}'s",
                place.label
            ),
        )
    })
}

fn port_number(port: u16) -> Result<NonZeroU16, Response> {
    NonZeroU16::new(port).ok_or_else(|| Response::error(ErrorCode::InvalidRequest, "Invalid port"))
}

pub(super) fn forward(
    view: &mut HerdrWindow,
    caller: &Caller,
    port: u16,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    let result = (|| {
        let place = Place::of(view, caller)?;
        let number = port_number(port)?;
        let target = forwarding_target(view, &place)?.to_owned();
        view.port_forwards
            .start(&target, number)
            .map_err(|error| Response::error(ErrorCode::Failed, error.to_string()))?;
        Ok(format!(
            "Forwarding port {port} of {}; `herdr-gpui ports` shows its local address",
            place.label
        ))
    })();
    match result {
        Ok(message) => {
            cx.notify();
            Response::done(message)
        }
        Err(response) => response,
    }
}

pub(super) fn unforward(
    view: &mut HerdrWindow,
    caller: &Caller,
    port: u16,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    let result = (|| {
        let place = Place::of(view, caller)?;
        let number = port_number(port)?;
        let target = forwarding_target(view, &place)?.to_owned();
        if !view.port_forwards.stop(&target, number) {
            return Err(Response::error(
                ErrorCode::NotFound,
                format!("Port {port} of {} is not forwarded", place.label),
            ));
        }
        Ok(format!("Stopped forwarding port {port} of {}", place.label))
    })();
    match result {
        Ok(message) => {
            cx.notify();
            Response::done(message)
        }
        Err(response) => response,
    }
}

/// What `ports open` does next: answered, or waiting for a forward of
/// `port` on `target` to listen before opening its page.
enum Opening {
    Answered(Response),
    Forwarding {
        place: Place,
        target: SshTarget,
        port: NonZeroU16,
    },
}

fn opened(port: u16, place: &Place, how: Option<PageOpened>) -> Response {
    super::page_answer(how, || {
        format!("Opened port {port} in workspace {}", place.workspace)
    })
}

fn start_open(
    view: &mut HerdrWindow,
    caller: &Caller,
    port: u16,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Result<Opening, Response> {
    let place = Place::of(view, caller)?;
    let number = port_number(port)?;
    // A listed port opens as clicking it in the status bar does, through a
    // tunnel when it listens only on its remote host's loopback.
    let link = view
        .endpoints
        .get(place.index)
        .and_then(|endpoint| Daemon::of(&endpoint.connection.target))
        .and_then(|daemon| {
            let listed = view.listening_ports.get(&daemon, &place.workspace)?;
            let port = listed.ports.iter().find(|listed| listed.number == port)?;
            port.link(listed.origin)
        });
    if let Some(link) = link {
        let how = view.open_port_link(&place.endpoint, &place.workspace, &link, window, cx);
        return Ok(Opening::Answered(opened(port, &place, how)));
    }
    if local(view, &place) {
        let url = WebUrl::try_from(format!("http://localhost:{port}/").as_str())
            .map_err(|error| Response::error(ErrorCode::InvalidUrl, error.to_string()))?;
        let how = view.open_workspace_page(&place.endpoint, &place.workspace, url, window, cx);
        return Ok(Opening::Answered(opened(port, &place, Some(how))));
    }
    let target = forwarding_target(view, &place)?.to_owned();
    let running = view
        .port_forwards
        .for_host(&target)
        .find(|forward| forward.remote_port() == number)
        .is_some_and(|forward| !matches!(forward.state(), State::Ended(_)));
    if !running {
        view.port_forwards
            .start(&target, number)
            .map_err(|error| Response::error(ErrorCode::Failed, error.to_string()))?;
        cx.notify();
    }
    Ok(Opening::Forwarding {
        place,
        target,
        port: number,
    })
}

/// Opens the forward's page once it listens: the answer then, or `None`
/// while SSH is still connecting.
fn open_forwarded(
    view: &mut HerdrWindow,
    place: &Place,
    target: &SshTarget,
    port: NonZeroU16,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Option<Response> {
    let Some(forward) = view
        .port_forwards
        .for_host(target)
        .find(|forward| forward.remote_port() == port)
    else {
        return Some(Response::error(
            ErrorCode::Failed,
            format!("The forward of port {port} stopped"),
        ));
    };
    match forward.state() {
        State::Starting => None,
        State::Ended(reason) => Some(Response::error(
            ErrorCode::Failed,
            format!("Port {port} forward ended: {reason}"),
        )),
        State::Listening { .. } => {
            let url = forward.url()?;
            let how = view.open_workspace_page(&place.endpoint, &place.workspace, url, window, cx);
            Some(opened(port.get(), place, Some(how)))
        }
    }
}

pub(super) fn open(
    caller: Caller,
    port: u16,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let handle = match window_for(&caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let started = handle
        .update(cx, |view, window, cx| {
            start_open(view, &caller, port, window, cx)
        })
        .unwrap_or_else(|_| Err(Response::error(ErrorCode::NoWindow, "The window closed")));
    let (place, target, number) = match started {
        Ok(Opening::Forwarding {
            place,
            target,
            port,
        }) => (place, target, port),
        Ok(Opening::Answered(response)) | Err(response) => return incoming.respond(response),
    };
    watches.add(
        incoming,
        Duration::from_secs(FORWARD_WAIT_SECONDS),
        move |cx| {
            handle
                .update(cx, |view, window, cx| {
                    open_forwarded(view, &place, &target, number, window, cx)
                })
                .unwrap_or_else(|_| Some(Response::error(ErrorCode::NoWindow, "The window closed")))
        },
        Response::error(
            ErrorCode::Timeout,
            format!("The forward of port {port} is still connecting; try `herdr-gpui ports` later"),
        ),
        cx,
    );
}

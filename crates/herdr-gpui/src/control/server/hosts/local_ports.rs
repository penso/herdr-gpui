//! `ports all` and `ports stop|pause|resume`: every port this computer's
//! user listens on, as the Ports section lists them. A request keeps the
//! section's scan running for a minute and waits for its first listing;
//! stopping and pausing only open the app's confirmation.

use super::{Watches, socket, window_for};
use crate::{
    HerdrWindow,
    control::protocol::{
        Caller, ErrorCode, Response,
        hosts::{LOCAL_PORTS_WAIT_SECONDS, LocalPortInfo, LocalPorts, PortAction, Reply},
    },
    local_ports::{DEMAND, Listener, Pending, SUPPORTED, home_relative},
};
use gpui::{App, Context, Window};
use std::time::{Duration, Instant, SystemTime};

/// `listener` as `ports all` lists it.
pub(super) fn info(
    listener: &Listener,
    workspace: Option<String>,
    now: SystemTime,
    home: Option<&std::path::Path>,
) -> LocalPortInfo {
    LocalPortInfo {
        port: listener.number,
        bind: listener.address(),
        process: listener.process.clone(),
        pid: listener.identity.pid,
        name: listener.name.clone(),
        command: listener.command.clone(),
        project: listener.project.clone(),
        cpu_percent: listener.cpu,
        memory_bytes: Some(listener.memory).filter(|memory| *memory > 0),
        system: listener.system,
        directory: listener
            .directory
            .as_deref()
            .map(|path| home_relative(path, home)),
        uptime_seconds: listener.uptime(now).map(|uptime| uptime.as_secs()),
        paused: listener.paused,
        workspace,
        url: listener.url().map(|url| url.as_str().to_owned()),
    }
}

/// The listing a scan begun after `requested` found, once it has
/// answered; a failed scan, as an error.
fn listing(view: &HerdrWindow, requested: Instant) -> Option<Result<&[Listener], Response>> {
    Some(
        view.local_ports
            .fresh_listing(requested)?
            .map_err(|error| Response::error(ErrorCode::Failed, error)),
    )
}

fn list(view: &HerdrWindow, requested: Instant) -> Option<Response> {
    let listeners = match listing(view, requested)? {
        Ok(listeners) => listeners,
        Err(response) => return Some(response),
    };
    let now = SystemTime::now();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let listening = listeners
        .iter()
        .map(|listener| {
            let workspace = view
                .local_port_owner(listener.number)
                .map(|owned| owned.label);
            info(listener, workspace, now, home.as_deref())
        })
        .collect();
    Some(Response::Host {
        reply: Reply::LocalPorts(Box::new(LocalPorts {
            listening,
            note: None,
        })),
    })
}

fn signal(
    view: &mut HerdrWindow,
    requested: Instant,
    port: u16,
    action: PortAction,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Option<Response> {
    let listeners: Vec<Listener> = match listing(view, requested)? {
        Ok(listeners) => listeners
            .iter()
            .filter(|listener| listener.number == port)
            .filter(|listener| action != PortAction::Resume || listener.paused)
            .cloned()
            .collect(),
        Err(response) => return Some(response),
    };
    if listeners.is_empty() {
        return Some(Response::error(
            ErrorCode::NotFound,
            if action == PortAction::Resume {
                format!("No paused process listens on port {port} of this computer")
            } else {
                format!("Nothing of yours listens on port {port} of this computer")
            },
        ));
    }
    let pending = Pending { action, listeners };
    let count = pending.victims().len();
    if action == PortAction::Resume {
        return Some(match view.local_ports.signal(action, pending.victims()) {
            Ok(()) => {
                cx.notify();
                Response::done(format!("Resuming {count} process(es) on port {port}"))
            }
            Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
        });
    }
    Some(if view.ask_to_signal_ports(pending, window, cx) {
        super::super::work::bring_forward(window, cx);
        Response::done(format!(
            "Asked to {} {count} process(es) on port {port}; nothing is signalled until the user confirms",
            action.name()
        ))
    } else {
        Response::error(
            ErrorCode::WindowBusy,
            "A dialog is open in the window; close it first",
        )
    })
}

/// Answers once a scan of this computer begun after the request has
/// answered, keeping the scan running meanwhile and asking a running one to
/// scan at once. Rows cached while the section was closed are never used.
fn wait(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
    mut answer: impl FnMut(
        &mut HerdrWindow,
        Instant,
        &mut Window,
        &mut Context<HerdrWindow>,
    ) -> Option<Response>
    + 'static,
) {
    let requested = Instant::now();
    let mut asked = false;
    if !SUPPORTED {
        return incoming.respond(Response::error(
            ErrorCode::Unsupported,
            "Listing this computer's ports needs macOS or Linux",
        ));
    }
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    watches.add(
        incoming,
        Duration::from_secs(LOCAL_PORTS_WAIT_SECONDS),
        move |cx| {
            handle
                .update(cx, |view, window, cx| {
                    view.local_ports.demand(Instant::now() + DEMAND);
                    // A worker not yet started scans as soon as it starts.
                    asked = asked || view.local_ports.refresh();
                    answer(view, requested, window, cx)
                })
                .unwrap_or_else(|_| Some(Response::error(ErrorCode::NoWindow, "The window closed")))
        },
        Response::error(
            ErrorCode::Timeout,
            "This computer's ports were not read in time; try again",
        ),
        cx,
    );
}

pub(super) fn all(
    caller: &Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    wait(caller, incoming, watches, cx, |view, requested, _, _| {
        list(view, requested)
    });
}

pub(super) fn act(
    caller: &Caller,
    port: u16,
    action: PortAction,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    if port == 0 {
        return incoming.respond(Response::error(ErrorCode::InvalidRequest, "Invalid port"));
    }
    wait(
        caller,
        incoming,
        watches,
        cx,
        move |view, requested, window, cx| signal(view, requested, port, action, window, cx),
    );
}

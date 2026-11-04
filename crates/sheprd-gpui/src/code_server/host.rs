//! VS Code on the SSH hosts the windows connect to, one server per host, so
//! a remote workspace's VS Code tab opens the workspace's own folder there.
//! The server the app starts on this computer cannot: the folder is on
//! another machine.
//!
//! Each host's server is a [`Supervisor`] whose child is one `ssh`: it runs
//! `serve.sh` on the host, which starts `code serve-web` on a private Unix
//! socket, and forwards a loopback port here to that socket. The token goes
//! to the host on the script's standard input, never on a command line, and
//! ending the `ssh` ends the server. VS Code's own command line is
//! downloaded to `~/.herdr-gpui/vscode` on the host the first time.
//!
//! Hosts get servers only when the app starts VS Code (`[code] mode =
//! "start"`) and its license was accepted, as for the local one. A server
//! runs until the app quits or the config stops starting VS Code.
// i18n: migrated
mod error;

pub use error::Error;

use super::{
    Startup,
    launcher::{POLL, QUIT_BUDGET, RETRY, reported},
    supervisor::{Plan, Report, Status, Supervisor, Timing},
    token,
};
use crate::{
    browser::EMBEDDED,
    config::{CodeConfig, CodeMode},
};
use gpui::{App, Global};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, TcpListener, ToSocketAddrs},
    num::NonZeroU16,
    path::PathBuf,
    process::{Command, ExitStatus},
    sync::Arc,
    time::Instant,
};

/// The script `ssh` runs on the host.
const SCRIPT: &str = include_str!("host/serve.sh");

/// The loopback ports a host's server is first tried on: one per host, the
/// same each launch, since VS Code keeps a page's state by its origin.
const PORTS: std::ops::Range<u16> = 20_000..30_000;

/// Whether SSH hosts get VS Code servers of their own for `code`: when the
/// app starts VS Code, in a build that shows pages. A config with only an
/// address uses that server everywhere, as before.
pub(crate) fn serves(code: &CodeConfig) -> bool {
    let mode = code.mode.unwrap_or(if code.url.is_some() {
        CodeMode::Address
    } else {
        CodeMode::Start
    });
    EMBEDDED && mode == CodeMode::Start
}

/// FNV-1a, which names a host the same way on every launch.
fn fingerprint(target: &str) -> u64 {
    target.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `target`'s own port in [`PORTS`].
fn port_of(target: &str) -> NonZeroU16 {
    let span = u64::from(PORTS.end - PORTS.start);
    let offset = u16::try_from(fingerprint(target) % span).unwrap_or_default();
    NonZeroU16::new(PORTS.start + offset).unwrap_or(NonZeroU16::MIN)
}

/// The name `target`'s VS Code pages load its server at. Browsers keep
/// cookies by name, not by port, so on `127.0.0.1` every server's
/// `vscode-tkn` cookie would go to every other: a host would be sent the
/// token of the server on this computer, or of another host. Each host gets
/// a name of its own under `.localhost`, which is always this computer, and
/// the same one each launch, since VS Code keeps a page's state by origin.
pub(super) fn page_host(target: &str) -> String {
    format!("vscode-{:016x}.localhost", fingerprint(target))
}

/// Whether the system's resolver sends `name` anywhere but this computer.
/// WebKit asks it for a page's address, so a name it maps elsewhere, as a
/// DNS server or VPN could, would carry the token there. No answer at all
/// is fine: WebView2 resolves `.localhost` itself, and an address that does
/// not resolve sends nothing. Blocking: the worker asks, never the UI.
pub(super) fn resolves_elsewhere(name: &str, port: NonZeroU16) -> bool {
    (name, port.get())
        .to_socket_addrs()
        .is_ok_and(|mut found| found.any(|address| !address.ip().is_loopback()))
}

/// `target`'s own port, when it is free right now; else any free one is used.
pub(super) fn preferred_port(target: &str) -> Option<NonZeroU16> {
    let port = port_of(target);
    TcpListener::bind((Ipv4Addr::LOCALHOST, port.get()))
        .ok()
        .map(|_| port)
}

/// Why the host's script stopped, by its exit status: `serve.sh` names its
/// own failures, and `ssh` exits 255 for its own.
pub(super) fn exited(status: ExitStatus) -> super::Error {
    let error = match status.code() {
        Some(2) => Error::Setup,
        Some(3) => Error::Unsupported,
        Some(4) => Error::NoDownloader,
        Some(5) => Error::Download,
        Some(6) => Error::Unpack,
        Some(255) => Error::Ssh,
        _ => return super::Error::Exited(status),
    };
    error.into()
}

/// A socket name no other start uses, so two apps on one host, or a server
/// left from an earlier start, never share one.
fn socket_name() -> super::error::Result<String> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(super::Error::TokenRandom)?;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!("sheprd-vscode-{hex}"))
}

/// The `ssh` that serves VS Code from the host the plan names on `port`.
fn command(plan: &Plan, port: NonZeroU16) -> crate::Result<Command> {
    let target = plan.host.as_deref().unwrap_or_default();
    Ok(sheprd_client::serve_command(
        target,
        port.get(),
        &socket_name()?,
        SCRIPT,
    )?)
}

fn start(plan: Plan) -> crate::Result<Supervisor> {
    Supervisor::start_with(
        plan,
        command,
        // Never the token: the app gave the server its token itself.
        super::version,
        // The port is the host's own; nothing is saved.
        |_| Ok(()),
        Timing::default(),
    )
}

/// Each host's own token file, beside the local server's, so no host can
/// use another's token, nor that of the server on this computer.
fn token_file(target: &str) -> super::error::Result<PathBuf> {
    let path = token::path()?;
    Ok(path.with_file_name(format!("vscode-token-{:016x}", fingerprint(target))))
}

#[derive(Default)]
struct Entry {
    server: Option<Supervisor>,
    /// The worker's last report, read on a timer so rendering never waits.
    report: Option<Report>,
}

/// The servers on the hosts, by SSH target.
pub(crate) struct Hosts {
    servers: HashMap<String, Entry>,
    quitting: bool,
    /// Starts a host's worker; tests start nothing.
    start: fn(Plan) -> crate::Result<Supervisor>,
    /// Where a host's token is kept; tests never touch the user's state.
    token_file: fn(&str) -> super::error::Result<PathBuf>,
}

impl Default for Hosts {
    fn default() -> Self {
        Self {
            servers: HashMap::new(),
            quitting: false,
            start,
            token_file,
        }
    }
}

impl Global for Hosts {}

impl Hosts {
    /// Installs the app's host servers, stopped when it quits. Native test
    /// modes never install them, so their remote workspaces use the address.
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
        cx.on_app_quit(|cx| {
            let hosts = cx.global_mut::<Self>();
            hosts.quitting = true;
            let servers: Vec<_> = hosts
                .servers
                .drain()
                .filter_map(|(_, entry)| entry.server)
                .collect();
            cx.background_executor().spawn(async move {
                // Each `ssh` is asked to stop at once; each is then given
                // what is left of the budget.
                for server in &servers {
                    server.stop();
                }
                for server in servers {
                    server.shutdown(QUIT_BUDGET);
                }
            })
        })
        .detach();
        if !EMBEDDED {
            return;
        }
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if cx.update(|cx| cx.global_mut::<Self>().poll()) {
                    cx.update(|cx| cx.refresh_windows());
                }
            }
        })
        .detach();
    }

    /// Where `target`'s server is for `code`, or `None` when hosts get no
    /// servers of their own, so its workspaces use the local one.
    pub(crate) fn startup(cx: &App, code: &CodeConfig, target: &str) -> Option<Startup> {
        let hosts = cx.try_global::<Self>().filter(|_| serves(code))?;
        if !code.license_accepted {
            return Some(Startup::Consent);
        }
        let entry = hosts.servers.get(target);
        Some(reported(
            entry.is_some_and(|entry| entry.server.is_some()),
            entry.and_then(|entry| entry.report.as_ref()),
        ))
    }

    /// Starts VS Code on `target` unless it runs there already, as when a
    /// page first needs it. Returns at once: the worker does the rest.
    pub(crate) fn want(cx: &mut App, code: &CodeConfig, target: &str) {
        let Some(hosts) = cx.try_global::<Self>() else {
            return;
        };
        if hosts.quitting || !serves(code) || !code.license_accepted {
            return;
        }
        if let Some(entry) = hosts.servers.get(target) {
            let waiting = matches!(
                &entry.report,
                Some(Report { status: Status::Failed { retry, .. }, .. }) if *retry > Instant::now()
            );
            if entry.server.is_some() || waiting {
                return;
            }
        }
        let plan = (hosts.token_file)(target).map(|token_file| Plan {
            program: PathBuf::from("ssh"),
            port: None,
            token_file,
            host: Some(target.to_owned()),
        });
        let hosts = cx.global_mut::<Self>();
        let started = plan.map_err(crate::Error::from).and_then(hosts.start);
        let entry = hosts.servers.entry(target.to_owned()).or_default();
        match started {
            Ok(server) => {
                tracing::info!(target, "Starting VS Code on a host");
                entry.server = Some(server);
                entry.report = None;
            }
            Err(error) => {
                tracing::warn!(%error, target, "Cannot start VS Code on a host");
                entry.report = Some(Report {
                    revision: entry
                        .report
                        .as_ref()
                        .map_or(0, |report| report.revision + 1),
                    status: Status::Failed {
                        error: Arc::new(error),
                        retry: Instant::now() + RETRY,
                    },
                    address: None,
                });
            }
        }
        cx.refresh_windows();
    }

    /// Whether `target`'s server runs right now: what a window checks just
    /// before it loads a page, which carries the token.
    pub(crate) fn alive(cx: &App, target: &str) -> bool {
        cx.try_global::<Self>()
            .and_then(|hosts| hosts.servers.get(target)?.server.as_ref())
            .is_some_and(Supervisor::alive)
    }

    /// Takes the workers' latest reports now, rather than at the next poll.
    pub(crate) fn refresh(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().poll();
        }
    }

    /// Stops every host's server once `code` no longer starts VS Code.
    pub(crate) fn sync(cx: &mut App, code: &CodeConfig) {
        let Some(hosts) = cx.try_global::<Self>() else {
            return;
        };
        if hosts.servers.is_empty() || (serves(code) && code.license_accepted) {
            return;
        }
        tracing::info!("Stopping VS Code on hosts: the config no longer starts it");
        cx.global_mut::<Self>().servers.clear();
    }

    /// Takes each worker's latest report. Returns whether one changed.
    fn poll(&mut self) -> bool {
        let mut changed = false;
        for entry in self.servers.values_mut() {
            let Some(report) = entry.server.as_ref().map(Supervisor::report) else {
                continue;
            };
            changed |= entry
                .report
                .as_ref()
                .is_none_or(|known| known.revision != report.revision);
            entry.report = Some(report);
        }
        changed
    }

    /// Host servers whose starts run nothing.
    #[cfg(test)]
    pub(crate) fn fixture(cx: &mut App) {
        cx.set_global(Self {
            start: Supervisor::idle,
            token_file: |target| {
                Ok(PathBuf::from(format!(
                    "vscode-token-{:016x}",
                    fingerprint(target)
                )))
            },
            ..Self::default()
        });
    }

    /// Stands in for `target`'s worker report, as the poll would take it. A
    /// running stand-in reports it too, for windows that read it live.
    // Only the tests that start VS Code, where pages show, use it.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn set_report(cx: &mut App, target: &str, report: Option<Report>) {
        let entry = cx
            .global_mut::<Self>()
            .servers
            .entry(target.to_owned())
            .or_default();
        if let (Some(server), Some(report)) = (&entry.server, &report) {
            server.report_as(report.status.clone(), report.address.clone());
        }
        entry.report = report;
    }

    /// The plan `target`'s worker was started with.
    #[cfg(test)]
    pub(crate) fn plan(cx: &App, target: &str) -> Option<Plan> {
        let entry = cx.try_global::<Self>()?.servers.get(target)?;
        Some(entry.server.as_ref()?.plan().clone())
    }
}

#[cfg(test)]
mod tests;

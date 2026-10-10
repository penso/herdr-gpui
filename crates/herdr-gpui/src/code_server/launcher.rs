//! The VS Code server the app starts, one for the whole app: VS Code's
//! command line is found once at launch, `code serve-web` starts the first
//! time a VS Code tab needs a page, and it stops when the
//! app quits. Windows and the settings page read [`Startup`], which says
//! where the VS Code tabs' server is or why it is not there yet.
//!
//! Builds that cannot show pages (Linux) find and start nothing.
use super::{
    cli,
    supervisor::{Plan, Report, Status, Supervisor},
    token,
};
use crate::{
    browser::{EMBEDDED, WebUrl},
    config::{CodeConfig, CodeMode},
};
use gpui::{App, Global};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// VS Code's server license, which `code serve-web` asks its user to accept.
pub(crate) const LICENSE_URL: &str = "https://aka.ms/vscode-server-license";
pub(crate) const PRIVACY_URL: &str = "https://privacy.microsoft.com/en-US/privacystatement";
pub(crate) const LICENSE_TERMS: &str = "VS Code's server is Microsoft software. Starting it \
     accepts the Visual Studio Code Server License Terms and the Microsoft Privacy Statement, \
     so the app starts it only once you accept them.";

/// How often the worker's state is read for the windows.
const POLL: Duration = Duration::from_millis(250);
/// GPUI gives every quit handler together 200 ms; the child gets most of it
/// to stop before it is killed.
const QUIT_BUDGET: Duration = Duration::from_millis(150);
/// How long a worker that could not start is left before trying again.
const RETRY: Duration = Duration::from_secs(5);

/// What is known of VS Code's command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Cli {
    #[default]
    Unknown,
    Finding,
    /// The program that runs `serve-web`.
    Found(PathBuf),
    Missing,
}

/// Where the VS Code tabs' server is, or why there is none yet.
#[derive(Clone, Debug)]
pub(crate) enum Startup {
    /// The configured address, which may be unset.
    Address,
    /// Looking for VS Code's command line.
    Finding,
    /// Starting VS Code was chosen, but it is not installed.
    Missing,
    /// Waiting for the user to accept VS Code's server license.
    Consent,
    /// Ready to start once a page needs VS Code.
    Idle,
    /// VS Code is starting; `url` is its address once made.
    Starting {
        url: Option<WebUrl>,
    },
    Ready {
        url: WebUrl,
    },
    /// It failed or stopped, and starts again shortly.
    Failed {
        error: Arc<crate::Error>,
        url: Option<WebUrl>,
    },
}

impl Startup {
    /// The address VS Code pages use, kept while the server restarts so
    /// its pages are not closed meanwhile.
    pub(crate) fn url<'a>(&'a self, code: &'a CodeConfig) -> Option<&'a WebUrl> {
        match self {
            Self::Address => code.url.as_ref(),
            Self::Ready { url } => Some(url),
            Self::Starting { url } | Self::Failed { url, .. } => url.as_ref(),
            Self::Finding | Self::Missing | Self::Consent | Self::Idle => None,
        }
    }

    /// Whether there is a server to ask before making a page.
    pub(crate) fn serving(&self) -> bool {
        matches!(self, Self::Address | Self::Ready { .. })
    }

    /// Whether VS Code is offered: once there is an address, or
    /// the app starts VS Code.
    pub(crate) fn offered(&self, code: &CodeConfig) -> bool {
        match self {
            Self::Address => code.url.is_some(),
            Self::Finding => false,
            _ => true,
        }
    }
}

/// The mode in effect: the saved one, else `address` for a config that has
/// an address, as before there was a choice, else `start` once VS Code is
/// found. `None` while that is not known yet.
pub(crate) fn mode(code: &CodeConfig, cli: &Cli) -> Option<CodeMode> {
    code.mode.or(match (&code.url, cli) {
        (Some(_), _) | (None, Cli::Missing) => Some(CodeMode::Address),
        (None, Cli::Found(_)) => Some(CodeMode::Start),
        (None, Cli::Unknown | Cli::Finding) => None,
    })
}

/// `running` says whether a worker was started, which may not have
/// reported yet.
fn startup(
    embedded: bool,
    code: &CodeConfig,
    cli: &Cli,
    running: bool,
    report: Option<&Report>,
) -> Startup {
    if !embedded {
        return Startup::Address;
    }
    match mode(code, cli) {
        None => return Startup::Finding,
        Some(CodeMode::Address) => return Startup::Address,
        Some(CodeMode::Start) => {}
    }
    match cli {
        Cli::Unknown | Cli::Finding => return Startup::Finding,
        Cli::Missing => return Startup::Missing,
        Cli::Found(_) if !code.license_accepted => return Startup::Consent,
        Cli::Found(_) => {}
    }
    let Some(report) = report else {
        return if running {
            Startup::Starting { url: None }
        } else {
            Startup::Idle
        };
    };
    let url = report.address.as_ref().map(|address| address.url.clone());
    match (&report.status, url) {
        (Status::Ready, Some(url)) => Startup::Ready { url },
        (Status::Starting | Status::Ready, url) => Startup::Starting { url },
        (Status::Failed { error, .. }, url) => Startup::Failed {
            error: error.clone(),
            url,
        },
    }
}

/// The server to run for `code`: only in start mode, with VS Code found and
/// its license accepted, in a build that shows pages.
fn plan(embedded: bool, code: &CodeConfig, cli: &Cli, token_file: PathBuf) -> Option<Plan> {
    let Cli::Found(program) = cli else {
        return None;
    };
    (embedded && code.license_accepted && mode(code, cli) == Some(CodeMode::Start)).then(|| Plan {
        program: program.clone(),
        port: code.port,
        token_file,
    })
}

pub(crate) struct Launcher {
    cli: Cli,
    server: Option<Supervisor>,
    /// The worker's last report, read on a timer so rendering never waits.
    report: Option<Report>,
    quitting: bool,
    /// Starts the worker; tests start nothing.
    start: fn(Plan) -> crate::Result<Supervisor>,
    /// Where the token is kept; tests never touch the user's state folder.
    token_file: fn() -> super::error::Result<PathBuf>,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            cli: Cli::default(),
            server: None,
            report: None,
            quitting: false,
            start: Supervisor::start,
            token_file: token::path,
        }
    }
}

impl Global for Launcher {}

impl Launcher {
    /// Installs the app's launcher and starts looking for VS Code. Native
    /// test modes never install it, so their windows use addresses alone.
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
        cx.on_app_quit(|cx| {
            let launcher = cx.global_mut::<Self>();
            launcher.quitting = true;
            let server = launcher.server.take();
            cx.background_executor().spawn(async move {
                if let Some(server) = server {
                    server.shutdown(QUIT_BUDGET);
                }
            })
        })
        .detach();
        if !EMBEDDED {
            return;
        }
        cx.global_mut::<Self>().cli = Cli::Finding;
        let found = cx.background_executor().spawn(async { cli::find() });
        cx.spawn(async move |cx| {
            let found = found.await;
            match &found {
                Some(program) => tracing::info!(program = %program.display(), "Found VS Code"),
                None => tracing::info!("VS Code is not installed"),
            }
            cx.update(|cx| {
                cx.global_mut::<Self>().cli = found.map_or(Cli::Missing, Cli::Found);
                cx.refresh_windows();
            });
            loop {
                cx.background_executor().timer(POLL).await;
                let changed = cx.update(|cx| cx.global_mut::<Self>().poll());
                if changed {
                    cx.update(|cx| cx.refresh_windows());
                }
            }
        })
        .detach();
    }

    /// Takes the worker's latest report now, rather than at the next poll:
    /// a window does this before asking the server anything, since a server
    /// that has stopped leaves its port to whoever takes it next.
    pub(crate) fn refresh(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().poll();
        }
    }

    /// Takes the worker's latest report. Returns whether it changed.
    fn poll(&mut self) -> bool {
        let Some(report) = self.server.as_ref().map(Supervisor::report) else {
            return false;
        };
        let changed = self
            .report
            .as_ref()
            .is_none_or(|known| known.revision != report.revision);
        self.report = Some(report);
        changed
    }

    /// Whether the server the app started runs right now. A window asks
    /// just before it loads a page, which carries the token: a server that
    /// stopped leaves its port to whoever takes it next.
    pub(crate) fn alive(cx: &App) -> bool {
        cx.try_global::<Self>()
            .and_then(|launcher| launcher.server.as_ref())
            .is_some_and(Supervisor::alive)
    }

    /// What is known of VS Code's command line, or `None` where the app
    /// never looks for it.
    pub(crate) fn cli(cx: &App) -> Option<&Cli> {
        cx.try_global::<Self>().map(|launcher| &launcher.cli)
    }

    /// Where the VS Code tabs' server is for `code`. Without a launcher, as in
    /// native test modes, VS Code counts as not installed.
    pub(crate) fn startup(cx: &App, code: &CodeConfig) -> Startup {
        match cx.try_global::<Self>() {
            Some(launcher) => startup(
                EMBEDDED,
                code,
                &launcher.cli,
                launcher.server.is_some(),
                launcher.report.as_ref(),
            ),
            None => startup(EMBEDDED, code, &Cli::Missing, false, None),
        }
    }

    /// Starts VS Code for `code` unless it runs already, as when a page
    /// first needs it. A changed command line or port starts it anew.
    /// Returns at once: the worker does the rest.
    pub(crate) fn want(cx: &mut App, code: &CodeConfig) {
        let Some(launcher) = cx.try_global::<Self>() else {
            return;
        };
        if launcher.quitting {
            return;
        }
        let token_file = match (launcher.token_file)() {
            Ok(path) => path,
            Err(error) => {
                cx.global_mut::<Self>().fail(error.into());
                return;
            }
        };
        let Some(plan) = plan(EMBEDDED, code, &launcher.cli, token_file) else {
            return;
        };
        let launcher = cx.global_mut::<Self>();
        if launcher.keeps(&plan) {
            return;
        }
        if launcher.server.is_none()
            && let Some(Report {
                status: Status::Failed { retry, .. },
                ..
            }) = &launcher.report
            && *retry > Instant::now()
        {
            return;
        }
        launcher.server = None;
        launcher.report = None;
        match (launcher.start)(plan) {
            Ok(server) => launcher.server = Some(server),
            Err(error) => launcher.fail(error),
        }
        cx.refresh_windows();
    }

    /// Stops VS Code once `code`, as loaded, no longer asks for it: a mode
    /// other than starting it, chosen in Settings or written in the config
    /// file, or a license no longer accepted. Returns at once.
    pub(crate) fn sync(cx: &mut App, code: &CodeConfig) {
        let Some(launcher) = cx.try_global::<Self>() else {
            return;
        };
        let wanted = code.license_accepted && mode(code, &launcher.cli) == Some(CodeMode::Start);
        if launcher.server.is_some() && !wanted {
            tracing::info!("Stopping VS Code: the config no longer starts it");
            Self::stop(cx);
        }
    }

    /// Stops VS Code. Returns at once; the worker stops the child.
    pub(crate) fn stop(cx: &mut App) {
        if cx.has_global::<Self>() {
            let launcher = cx.global_mut::<Self>();
            launcher.server = None;
            launcher.report = None;
        }
    }

    /// Whether the running server is the one `plan` asks for. A plan without
    /// a port takes any; one with a port takes the server on it, such as the
    /// one that picked and saved it.
    fn keeps(&self, plan: &Plan) -> bool {
        let Some(server) = &self.server else {
            return false;
        };
        let running = server.plan();
        // Read now, not at the last poll: the config that brings back the
        // port the worker saved can arrive before the poll that saw it.
        let port = server
            .report()
            .address
            .map(|address| address.port)
            .or(running.port);
        running.program == plan.program
            && running.token_file == plan.token_file
            && (plan.port.is_none() || plan.port == port)
    }

    fn fail(&mut self, error: crate::Error) {
        tracing::warn!(%error, "Cannot start VS Code");
        self.server = None;
        self.report = Some(Report {
            revision: self.report.as_ref().map_or(0, |report| report.revision + 1),
            status: Status::Failed {
                error: Arc::new(error),
                retry: Instant::now() + RETRY,
            },
            address: None,
        });
    }

    /// A launcher that found `cli`, as a running app's would, whose starts
    /// run nothing.
    #[cfg(test)]
    pub(crate) fn fixture(cx: &mut App, cli: Cli) {
        cx.set_global(Self {
            cli,
            start: Supervisor::idle,
            token_file: || Ok(PathBuf::from("vscode-token")),
            ..Self::default()
        });
    }

    /// Whether a worker was started.
    #[cfg(test)]
    pub(crate) fn running(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|launcher| launcher.server.is_some())
    }

    /// The worker's report as a window would see it now.
    // Only the tests that start VS Code, where pages show, use it.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn report(cx: &App) -> Option<Report> {
        cx.try_global::<Self>()?.report.clone()
    }

    /// The stand-in worker's child exits, before its report says so.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn stand_in_exits(cx: &App) {
        if let Some(server) = &cx.global::<Self>().server {
            server.stand_in_exits();
        }
    }

    /// Stands in for the worker's report, as the poll would take it.
    // Only the tests that start VS Code, where pages show, use it.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn set_report(cx: &mut App, report: Option<Report>) {
        let launcher = cx.global_mut::<Self>();
        // A running stand-in reports it too, for windows that read it live.
        if let (Some(server), Some(report)) = (&launcher.server, &report) {
            server.report_as(report.status.clone(), report.address.clone());
        }
        launcher.report = report;
    }
}

#[cfg(test)]
mod tests;

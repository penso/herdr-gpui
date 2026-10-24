//! The caller's side: build a request from the command line, send it to
//! the running app (or read the app's files), and print the answer as text
//! or, with `--json`, as one JSON document.
use crate::control::ControlError;
mod app;
mod browser;
mod chat;
mod events;
mod help;
mod hosts;
mod local;
mod open;
mod remote;
mod settings;
mod tabs;
mod work;

use super::{
    ErrorCode,
    protocol::{Call, Caller, Request, Response},
};
use super::{socket, socket_path};
use crate::cli::control::{BrowserCommand, ControlCommand, Invocation};
use serde::Serialize;
use std::{
    io::{IsTerminal as _, Write as _},
    process::ExitCode,
};

/// Exit statuses, the same for every command. 0 is done.
pub(crate) mod exit {
    /// Refused or failed.
    pub const REFUSED: u8 = 1;
    /// Invalid arguments, or an address or file that cannot be shown.
    pub const USAGE: u8 = 2;
    /// No Herdr UI is listening, so a caller can fall back to printing.
    pub const NOT_RUNNING: u8 = 3;
    /// Nothing yet: no notes, no event, or the condition waited for.
    pub const NOTHING: u8 = 4;
    /// The window, workspace, tab, host, or file named is not there.
    pub const NOT_FOUND: u8 = 5;
    /// A dialog holds the window, or the app is busy with other requests.
    pub const BUSY: u8 = 6;
    /// The app did not answer in time.
    pub const TIMEOUT: u8 = 7;
    /// This platform or this version of the app cannot do it.
    pub const UNSUPPORTED: u8 = 8;
}

/// What a command printed, and how it exits.
pub(crate) struct Output {
    text: String,
    json: serde_json::Value,
    status: u8,
    /// Output whose bytes matter, such as a patch: written unchanged into a
    /// pipe or file, and shown from `text` on a terminal.
    bytes: Option<Vec<u8>>,
    /// Text for standard error besides, such as a remote command's, which
    /// `--json` carries in `json` instead.
    stderr: String,
}

impl Output {
    /// A message for a person; JSON carries it as `{"message": ...}`.
    pub(crate) fn message(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            json: serde_json::json!({ "message": text }),
            text,
            status: 0,
            bytes: None,
            stderr: String::new(),
        }
    }

    /// `text` for a person, `data` for a program.
    pub(crate) fn data(text: impl Into<String>, data: &impl Serialize) -> crate::Result<Self> {
        Ok(Self {
            text: text.into(),
            json: serde_json::to_value(data)?,
            status: 0,
            bytes: None,
            stderr: String::new(),
        })
    }

    /// `bytes` exactly, for a program reading a pipe; `data` for `--json`.
    pub(crate) fn bytes(bytes: Vec<u8>, data: &impl Serialize) -> crate::Result<Self> {
        Ok(Self {
            text: String::from_utf8_lossy(&bytes).trim_end().to_owned(),
            json: serde_json::to_value(data)?,
            status: 0,
            bytes: Some(bytes),
            stderr: String::new(),
        })
    }

    /// Done, but with nothing to report: exits [`exit::NOTHING`].
    pub(crate) fn nothing(mut self) -> Self {
        self.status = exit::NOTHING;
        self
    }

    /// An answer that reports a failure, such as a batch where a device
    /// failed: printed in full, exiting [`exit::REFUSED`].
    pub(crate) fn failed(mut self) -> Self {
        self.status = exit::REFUSED;
        self
    }

    /// Also print `text` on standard error, unless `--json` asked.
    pub(crate) fn with_stderr(mut self, text: impl Into<String>) -> Self {
        self.stderr = text.into();
        self
    }
}

/// Who asks and where: the pane, workspace, and daemon Herdr set in the
/// caller's environment, and the window `--window` names.
pub(crate) struct Context {
    pub json: bool,
    pub window: Option<u64>,
}

impl Context {
    pub(crate) fn caller(&self) -> crate::Result<Caller> {
        Ok(Caller {
            window: self.window,
            ..caller(None)?
        })
    }

    /// Sends `op` of a family and returns the app's answer, an error answer
    /// already turned into an error.
    pub(crate) fn call<T>(
        &self,
        family: impl FnOnce(Call<T>) -> Request,
        op: T,
    ) -> crate::Result<Response> {
        let request = family(Call {
            caller: self.caller()?,
            op,
        });
        match send(&request)? {
            Response::Error { code, message } => {
                Err(crate::Error::Control(ControlError::Rejected {
                    code,
                    message,
                }))
            }
            response => Ok(response),
        }
    }

    /// For operations whose answer is only `Done`.
    pub(crate) fn act<T>(
        &self,
        family: impl FnOnce(Call<T>) -> Request,
        op: T,
    ) -> crate::Result<Output> {
        match self.call(family, op)? {
            Response::Done { message } => Ok(Output::message(message)),
            response => Err(unexpected(response)),
        }
    }
}

/// The daemon a pane's agent belongs to, from the variables Herdr sets in it.
fn caller_daemon() -> crate::Result<Option<String>> {
    if std::env::var_os("HERDR_SOCKET_PATH").is_none() {
        return Ok(None);
    }
    let path = herdr_client::ConnectTarget::Local.socket_path()?;
    Ok(path.to_str().map(str::to_owned))
}

/// Notes must never fall back to pane-only routing, even in the default session.
fn notes_daemon() -> crate::Result<String> {
    let path = herdr_client::ConnectTarget::Local.socket_path()?;
    path.into_os_string().into_string().map_err(|_| {
        crate::Error::Control(ControlError::Rejected {
            code: ErrorCode::InvalidRequest,
            message: "The daemon socket must be UTF-8 for control requests".into(),
        })
    })
}

pub(super) fn env_id(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|id| !id.is_empty())
}

fn caller(workspace: Option<String>) -> crate::Result<Caller> {
    Ok(Caller {
        pane_id: env_id("HERDR_PANE_ID"),
        workspace_id: workspace.or_else(|| env_id("HERDR_WORKSPACE_ID")),
        daemon_socket: caller_daemon()?,
        window: None,
    })
}

fn send(request: &Request) -> crate::Result<Response> {
    Ok(socket::call(&socket_path()?, request)?)
}

/// Hands a `herdr-gpui://` link to the running app; what it did, for a
/// person. [`ControlError::Unavailable`](ControlError::Unavailable) means no app is running.
pub(crate) fn send_link(link: &crate::link::Link) -> crate::Result<String> {
    hand_off(&Request::Link(Call {
        caller: Caller::default(),
        op: super::protocol::LinkOp { url: link.to_url() },
    }))
}

/// Brings the running app's window forward, for a plain launch while it
/// runs; what it did, for a person. [`ControlError::Unavailable`](ControlError::Unavailable)
/// means no app is running.
#[cfg(windows)]
pub(crate) fn send_focus() -> crate::Result<String> {
    hand_off(&focus_request())
}

/// `app focus` for no workspace in particular: the frontmost window.
#[cfg(any(windows, test))]
fn focus_request() -> Request {
    Request::App(Call {
        caller: Caller::default(),
        op: super::protocol::app::AppOp::Focus,
    })
}

/// Sends what a launch hands to the running app instead of starting it.
fn hand_off(request: &Request) -> crate::Result<String> {
    // The shell lets the process it started for a click take the
    // foreground; this passes that on to the app, so its window comes
    // forward instead of flashing in the taskbar.
    #[cfg(windows)]
    allow_foreground();
    match send(request)? {
        Response::Done { message } => Ok(message),
        response => Err(rejected(response)),
    }
}

// One documented call with no pointers; windows-sys and `windows` have no
// safe wrapper for it.
#[cfg(windows)]
#[allow(unsafe_code)]
fn allow_foreground() {
    use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
    // SAFETY: takes a process ID and returns whether it applied. It does
    // nothing for a process without the foreground right to pass on.
    if let Err(error) = unsafe { AllowSetForegroundWindow(ASFW_ANY) } {
        tracing::debug!(%error, "foreground not passed to the app");
    }
}

fn rejected(response: Response) -> crate::Error {
    match response {
        Response::Error { code, message } => {
            crate::Error::Control(ControlError::Rejected { code, message })
        }
        response => unexpected(response),
    }
}

/// An answer of another kind than asked for: an app of another version.
pub(crate) fn unexpected(response: Response) -> crate::Error {
    tracing::debug!(?response, "Unexpected control response");
    crate::Error::Control(ControlError::NoResponse)
}

/// Sends the user's notes to the agent in this process's Herdr pane, through
/// the running Herdr UI. Blocking: call it off the UI thread.
#[cfg(feature = "mockup")]
pub(crate) fn send_notes(text: String) -> crate::Result<super::NotesTo> {
    let mut caller = caller(None)?;
    if caller.pane_id.is_none() {
        return Err(crate::Error::Control(ControlError::Rejected {
            code: ErrorCode::InvalidRequest,
            message: "Not running in a Herdr pane".into(),
        }));
    }
    caller.daemon_socket = Some(notes_daemon()?);
    match send(&Request::Notes(super::protocol::NotesRequest {
        caller,
        text,
    }))? {
        Response::NotesSent { to } => Ok(to),
        response => Err(rejected(response)),
    }
}

/// The exit status an error maps to.
pub(crate) fn status(error: &crate::Error) -> u8 {
    match error {
        crate::Error::InvalidBrowserUrl | crate::Error::InvalidLocalPage => exit::USAGE,
        crate::Error::Control(ControlError::Unavailable { .. }) => exit::NOT_RUNNING,
        crate::Error::Control(ControlError::Rejected { code, .. }) => match code {
            ErrorCode::InvalidUrl => exit::USAGE,
            ErrorCode::NotFound | ErrorCode::WorkspaceNotFound | ErrorCode::NoWindow => {
                exit::NOT_FOUND
            }
            ErrorCode::Busy | ErrorCode::WindowBusy | ErrorCode::TabLimit => exit::BUSY,
            ErrorCode::Timeout => exit::TIMEOUT,
            ErrorCode::Unsupported => exit::UNSUPPORTED,
            ErrorCode::InvalidRequest | ErrorCode::Failed => exit::REFUSED,
        },
        _ => exit::REFUSED,
    }
}

/// The error's code as JSON names it.
fn code(error: &crate::Error) -> &'static str {
    match status(error) {
        exit::USAGE => "usage",
        exit::NOT_RUNNING => "not_running",
        exit::NOT_FOUND => "not_found",
        exit::BUSY => "busy",
        exit::TIMEOUT => "timeout",
        exit::UNSUPPORTED => "unsupported",
        _ => "refused",
    }
}

/// Text for the caller's terminal: lines and tabs stay, every other control
/// or direction override goes, as app and terminal text is untrusted.
pub(crate) fn printable(text: &str, limit: usize) -> String {
    text.chars()
        .take(limit)
        .filter(|c| matches!(c, '\n' | '\t') || !crate::notifications::unsafe_char(*c))
        .collect()
}

/// Successful output in full: a terminal gets it without controls, which
/// could drive the terminal, and a pipe or file gets it unchanged, so a
/// patch from `checkpoint diff` applies byte for byte. App answers are
/// already bounded by the socket's message limit.
fn for_terminal(text: &str, terminal: bool) -> std::borrow::Cow<'_, str> {
    if terminal {
        printable(text, usize::MAX).into()
    } else {
        text.into()
    }
}

/// Prints the result and gives the exit status. Text that came from the
/// app or the terminal is made safe before it reaches the caller's terminal.
fn finish(result: crate::Result<Output>, json: bool) -> ExitCode {
    match result {
        Ok(output) if json => {
            println!("{}", output.json);
            ExitCode::from(output.status)
        }
        Ok(output) => {
            let status = print(&output);
            if !output.stderr.is_empty() {
                eprint!(
                    "{}",
                    for_terminal(&output.stderr, std::io::stderr().is_terminal())
                );
            }
            ExitCode::from(status)
        }
        Err(error) => {
            let message = crate::notifications::safe_text(&error.to_string(), 4096);
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "error": { "code": code(&error), "message": message } })
                );
            } else {
                eprintln!("{message}");
            }
            ExitCode::from(status(&error))
        }
    }
}

/// Prints successful output as text; the exit status.
fn print(output: &Output) -> u8 {
    if let Some(bytes) = &output.bytes
        && !std::io::stdout().is_terminal()
    {
        let mut stdout = std::io::stdout().lock();
        if let Err(error) = stdout.write_all(bytes).and_then(|()| stdout.flush()) {
            eprintln!("{error}");
            return exit::REFUSED;
        }
        return output.status;
    }
    if !output.text.is_empty() {
        if output.status == exit::NOTHING {
            eprintln!(
                "{}",
                for_terminal(&output.text, std::io::stderr().is_terminal())
            );
        } else {
            println!(
                "{}",
                for_terminal(&output.text, std::io::stdout().is_terminal())
            );
        }
    }
    output.status
}

/// Runs a control command without starting GPUI.
pub(crate) fn run(invocation: Invocation) -> ExitCode {
    let context = Context {
        json: invocation.json,
        window: invocation.window,
    };
    let result = match invocation.command {
        ControlCommand::Help(family) => {
            println!("{}", help::usage(family));
            return ExitCode::SUCCESS;
        }
        ControlCommand::Browser(BrowserCommand::Help) => {
            println!("{}", help::usage(Some("browser")));
            return ExitCode::SUCCESS;
        }
        ControlCommand::Skill | ControlCommand::Browser(BrowserCommand::Skill) => {
            print!(
                "{}",
                crate::agent_skill::text(crate::agent_skill::executable().as_deref())
            );
            return ExitCode::SUCCESS;
        }
        ControlCommand::Browser(BrowserCommand::Example) => {
            print!("{}", browser::EXAMPLE);
            return ExitCode::SUCCESS;
        }
        ControlCommand::Browser(command) => browser::run(command, &context),
        ControlCommand::Status => app::status(&context),
        ControlCommand::App(command) => app::app(command, &context),
        ControlCommand::Open { path } => open::open(&path, &context),
        ControlCommand::Window(command) => app::window(command, &context),
        ControlCommand::Ui(command) => app::ui(command, &context),
        ControlCommand::Run { name } => app::run_command(&name, &context),
        ControlCommand::Commands => local::commands(),
        ControlCommand::Tab(command) => tabs::tab(command, &context),
        ControlCommand::Code(command) => tabs::code(command, &context),
        ControlCommand::Devices => tabs::devices(&context),
        ControlCommand::Orchestrator(command) => tabs::orchestrator(command, &context),
        ControlCommand::Review => tabs::review(&context),
        ControlCommand::Chat(command) => chat::chat(command, &context),
        ControlCommand::Px0 => tabs::px0(&context),
        ControlCommand::Notes(command) => work::notes(command, &context),
        ControlCommand::Feedback { wait } => browser::feedback(wait, &context),
        ControlCommand::Checkpoint(command) => work::checkpoint(command, &context),
        ControlCommand::Scripts { kind } => work::scripts(kind, &context),
        ControlCommand::Teleport { worktree, host } => {
            work::teleport(worktree.as_deref(), host, &context)
        }
        ControlCommand::FanOut(fan_out) => work::fan_out(fan_out, &context),
        ControlCommand::Pr(command) => hosts::pr(command, &context),
        ControlCommand::Host(command) => hosts::host(command, &context),
        ControlCommand::Cloud(command) => hosts::cloud(command, &context),
        ControlCommand::Ports(command) => hosts::ports(command, &context),
        ControlCommand::Processes { pane } => local::processes(pane.as_deref()),
        ControlCommand::Usage => hosts::usage(&context),
        ControlCommand::System => hosts::system(&context),
        ControlCommand::Security(command) => hosts::security(command, &context),
        ControlCommand::Automation(command) => hosts::automation(command, &context),
        ControlCommand::Remote(command) => remote::remote(command, &context),
        ControlCommand::Search {
            files,
            query,
            limit,
        } => local::search(files, &query, limit),
        ControlCommand::Config(command) => settings::config(command, &context),
        ControlCommand::Theme(command) => settings::theme(command, &context),
        ControlCommand::Font(command) => settings::font(command, &context),
        ControlCommand::Keybinds(command) => settings::keybinds(command, &context),
        ControlCommand::Caffeine(command) => settings::caffeine(command, &context),
        ControlCommand::Sound(command) => settings::sound(command, &context),
        ControlCommand::Bell => settings::bell(&context),
        ControlCommand::Events {
            after,
            follow,
            wait,
            kinds,
        } => events::events(after, follow, wait, kinds, &context),
        ControlCommand::Wait { what, timeout } => events::wait(what, timeout, &context),
        ControlCommand::Logs { path, lines } => local::logs(path, lines),
        ControlCommand::Update(command) => settings::update(command, &context),
        ControlCommand::Diagnostics { output } => local::diagnostics(&output),
        ControlCommand::Integrations(command) => settings::integrations(command, &context),
    };
    finish(result, context.json)
}

#[cfg(test)]
mod tests;

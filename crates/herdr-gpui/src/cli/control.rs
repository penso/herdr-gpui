//! `herdr-gpui <command> ...`: commands answered by the running app, or by
//! this process from the app's own files, without starting GPUI. Herdr's own
//! CLI already drives workspaces, tabs, panes, agents, and notifications;
//! these cover what only this client knows or draws.
mod app;
mod browser;
mod hosts;
mod settings;
mod tabs;
mod work;

use super::CliError;
pub(crate) use super::args::Args;
pub use crate::control::{Cursor, EventKind, FontSize, Script, Size, Sound, Switch};
pub use app::{AppCommand, UiCommand, WindowCommand};
pub use browser::{BrowserCommand, Placement};
pub use hosts::{CloudCommand, HostCommand, PortsCommand, PrCommand};
pub use settings::{
    CaffeineCommand, ConfigCommand, IntegrationsCommand, KeybindsCommand, SoundCommand,
    ThemeCommand, UpdateCommand,
};
use std::ffi::OsString;
pub use tabs::{CodeCommand, FileAt, Items, OrchestratorCommand, TabCommand};
pub use work::{CheckpointCommand, FanOut, NotesCommand, WaitFor};

/// What to do, and how the answer is printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub command: ControlCommand,
    /// Print the answer as one JSON document instead of text.
    pub json: bool,
    /// The window to act in, by the ID `window list` prints. Absent, the
    /// window showing the caller's workspace (none: refused), or outside
    /// Herdr the frontmost one.
    pub window: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    /// Usage for every command, or for one family.
    Help(Option<&'static str>),
    Status,
    Skill,
    Browser(BrowserCommand),
    App(AppCommand),
    /// Open a folder as the palette's Open Project does.
    Open {
        path: std::path::PathBuf,
    },
    Window(WindowCommand),
    Ui(UiCommand),
    Tab(TabCommand),
    Code(CodeCommand),
    Devices,
    Orchestrator(OrchestratorCommand),
    Review,
    Notes(NotesCommand),
    /// Every kind of note the user sent this pane: page, review, terminal.
    Feedback {
        wait: u64,
    },
    Checkpoint(CheckpointCommand),
    /// List the scripts, or run one.
    Scripts {
        kind: Option<Script>,
    },
    Teleport {
        worktree: Option<String>,
        host: String,
    },
    FanOut(FanOut),
    Pr(PrCommand),
    Host(HostCommand),
    Cloud(CloudCommand),
    Ports(PortsCommand),
    Processes {
        pane: Option<String>,
    },
    Usage,
    System,
    Search {
        files: bool,
        query: String,
        limit: usize,
    },
    Config(ConfigCommand),
    Theme(ThemeCommand),
    Font(ThemeCommand),
    Keybinds(KeybindsCommand),
    Caffeine(CaffeineCommand),
    Sound(SoundCommand),
    Bell,
    Events {
        after: Option<Cursor>,
        follow: bool,
        wait: u64,
        kinds: Vec<EventKind>,
    },
    Wait {
        what: WaitFor,
        timeout: u64,
    },
    /// Run a command by its `[keybindings]` name, as the palette does.
    Run {
        name: String,
    },
    Commands,
    Logs {
        path: bool,
        lines: usize,
    },
    Update(UpdateCommand),
    Diagnostics {
        output: std::path::PathBuf,
    },
    Integrations(IntegrationsCommand),
}

/// The words that start a control command, with a one-line summary each,
/// for the top-level help.
pub const FAMILIES: &[(&str, &str)] = &[
    (
        "status",
        "Whether Herdr UI runs, its version, windows, and hosts",
    ),
    ("skill", "Print the agent skill describing these commands"),
    (
        "browser",
        "Browser tabs: open, reload, list, navigate, page notes",
    ),
    ("app", "Bring the app forward, hide it, or quit it"),
    (
        "open",
        "Open a folder in Herdr UI, starting the app if needed",
    ),
    (
        "window",
        "List, open, focus, resize, detach, or reconnect windows",
    ),
    ("ui", "Sidebar, status bar, and terminal font size"),
    ("tab", "List, focus, or close the app's own tabs"),
    (
        "code",
        "Show a file in a code tab or open it in your editor",
    ),
    ("devices", "Open the Devices overview"),
    (
        "orchestrator",
        "A repository's issues, pull requests, and runs",
    ),
    ("review", "Open the review of the uncommitted changes"),
    ("notes", "Notes kept on worktrees"),
    (
        "feedback",
        "Notes the user sent this pane, from pages, reviews, or terminals",
    ),
    (
        "checkpoint",
        "Agent checkpoints of a checkout: list, take, diff, restore",
    ),
    (
        "scripts",
        "A repository's worktree setup, run, and archive scripts",
    ),
    ("teleport", "Move a worktree to another host"),
    (
        "fan-out",
        "Send one prompt to several agents, each in its own worktree",
    ),
    ("pr", "The pull request of the caller's branch"),
    ("host", "List the window's hosts, or show one"),
    ("cloud", "Saved cloud machines, and adding one"),
    (
        "ports",
        "Listening ports, and forwarding them to this machine",
    ),
    ("processes", "The processes running under a pane"),
    ("usage", "AI plan usage on the selected host"),
    ("system", "The selected host's load"),
    ("search", "Find a symbol or a file in the focused checkout"),
    ("config", "Herdr UI settings: path, get, set, reload, check"),
    ("theme", "List or choose a color theme"),
    ("font", "List or choose the terminal font"),
    ("keybinds", "List shortcuts and check them for conflicts"),
    ("caffeine", "Keep the machine awake"),
    ("sound", "List or play notification sounds"),
    ("bell", "Ring the terminal bell"),
    ("events", "What happened in the app, as a stream"),
    ("wait", "Wait for feedback, a window, or a tab to close"),
    (
        "run",
        "Run any app command by name, as the command palette does",
    ),
    ("commands", "List the commands `run` accepts"),
    ("logs", "Print or locate the app's log"),
    ("update", "Check for an app update; never installs"),
    ("diagnostics", "Write a redacted bundle for a bug report"),
    ("integrations", "Agent integrations and the agent skill"),
];

/// Whether `args` are a control command asking for JSON, whose errors,
/// usage errors included, are then printed as JSON.
pub fn wants_json(args: &[OsString]) -> bool {
    args.first().is_some_and(is_family) && args.iter().any(|arg| arg == "--json")
}

pub fn is_family(word: &OsString) -> bool {
    FAMILIES.iter().any(|(name, _)| word == *name)
}

/// The longest wait any command accepts, in seconds.
pub const MAX_WAIT: u64 = 600;

fn switch(word: Option<OsString>) -> Result<Switch, CliError> {
    match word.as_ref().and_then(|word| word.to_str()) {
        None => Ok(Switch::Toggle),
        Some("on" | "show") => Ok(Switch::On),
        Some("off" | "hide") => Ok(Switch::Off),
        Some("toggle") => Ok(Switch::Toggle),
        Some(_) => Err(CliError::InvalidValue("on|off|toggle")),
    }
}

fn wait_option(args: &mut Args, flag: &'static str) -> Result<u64, CliError> {
    Ok(args.number(flag, 0..=MAX_WAIT)?.unwrap_or(0))
}

/// A verb that names no subcommand of `family`.
fn unknown(family: &'static str, verb: OsString) -> CliError {
    CliError::UnknownCommand {
        family,
        command: verb,
    }
}

/// Parses everything after the family word. `--json`, `--window ID`, and
/// `--help` may appear anywhere.
pub fn parse(
    family: OsString,
    rest: impl Iterator<Item = OsString>,
) -> Result<Invocation, CliError> {
    let family = FAMILIES
        .iter()
        .map(|(name, _)| *name)
        .find(|name| family == *name)
        .ok_or(CliError::UnknownOption(family))?;
    let mut args = Args::new(rest, family);
    // Browser help stays as it was: any `--help` prints the browser usage.
    if family == "browser" {
        let json = args.flag("--json");
        let window = args.number("--window", 0..=u64::MAX)?;
        return Ok(Invocation {
            command: ControlCommand::Browser(browser::parse(&mut args)?),
            json,
            window,
        });
    }
    if args.wants_help() {
        return Ok(Invocation {
            command: ControlCommand::Help(Some(family)),
            json: false,
            window: None,
        });
    }
    let json = args.flag("--json");
    let window = args.number("--window", 0..=u64::MAX)?;
    let command = command(family, &mut args)?;
    args.finish()?;
    Ok(Invocation {
        command,
        json,
        window,
    })
}

fn command(family: &'static str, args: &mut Args) -> Result<ControlCommand, CliError> {
    Ok(match family {
        "status" => ControlCommand::Status,
        "skill" => ControlCommand::Skill,
        "app" => ControlCommand::App(app::app(args)?),
        "open" => ControlCommand::Open {
            path: args.required("DIR")?.into(),
        },
        "window" => ControlCommand::Window(app::window(args)?),
        "ui" => ControlCommand::Ui(app::ui(args)?),
        "tab" => ControlCommand::Tab(tabs::tab(args)?),
        "code" => ControlCommand::Code(tabs::code(args)?),
        "devices" => {
            tabs::open_only(args)?;
            ControlCommand::Devices
        }
        "orchestrator" => ControlCommand::Orchestrator(tabs::orchestrator(args)?),
        "review" => {
            tabs::open_only(args)?;
            ControlCommand::Review
        }
        "notes" => ControlCommand::Notes(work::notes(args)?),
        "feedback" => ControlCommand::Feedback {
            wait: wait_option(args, "--wait")?,
        },
        "checkpoint" => ControlCommand::Checkpoint(work::checkpoint(args)?),
        "scripts" => ControlCommand::Scripts {
            kind: work::script_kind(args)?,
        },
        "teleport" => ControlCommand::Teleport {
            host: args
                .text_option("--to")?
                .ok_or(CliError::MissingValue("--to"))?,
            worktree: args.optional_text()?,
        },
        "fan-out" => ControlCommand::FanOut(work::fan_out(args)?),
        "pr" => ControlCommand::Pr(hosts::pr(args)?),
        "host" => ControlCommand::Host(hosts::host(args)?),
        "cloud" => ControlCommand::Cloud(hosts::cloud(args)?),
        "ports" => ControlCommand::Ports(hosts::ports(args)?),
        "processes" => ControlCommand::Processes {
            pane: args.text_option("--pane")?,
        },
        "usage" => ControlCommand::Usage,
        "system" => ControlCommand::System,
        "search" => {
            let files = match args.verb().as_ref().and_then(|verb| verb.to_str()) {
                Some("symbol" | "symbols") => false,
                Some("file" | "files") => true,
                _ => return Err(CliError::InvalidValue("symbol|file")),
            };
            ControlCommand::Search {
                limit: args.number("--limit", 1..=200)?.unwrap_or(20),
                query: args.words()?.ok_or(CliError::MissingArgument {
                    command: "search",
                    what: "QUERY",
                })?,
                files,
            }
        }
        "config" => ControlCommand::Config(settings::config(args)?),
        "theme" => ControlCommand::Theme(settings::theme(args)?),
        "font" => ControlCommand::Font(settings::theme(args)?),
        "keybinds" => ControlCommand::Keybinds(settings::keybinds(args)?),
        "caffeine" => ControlCommand::Caffeine(settings::caffeine(args)?),
        "sound" => ControlCommand::Sound(settings::sound(args)?),
        "bell" => ControlCommand::Bell,
        "events" => {
            let kinds = args
                .options("--kind")?
                .iter()
                .map(|kind| {
                    kind.to_str()
                        .and_then(|kind| kind.parse().ok())
                        .ok_or(CliError::InvalidValue("--kind"))
                })
                .collect::<Result<_, _>>()?;
            ControlCommand::Events {
                follow: args.flag("--follow"),
                after: args
                    .text_option("--after")?
                    .map(|after| {
                        after
                            .parse()
                            .map_err(|()| CliError::InvalidValue("--after"))
                    })
                    .transpose()?,
                wait: wait_option(args, "--wait")?,
                kinds,
            }
        }
        "wait" => ControlCommand::Wait {
            timeout: args.number("--timeout", 1..=MAX_WAIT)?.unwrap_or(60),
            what: work::wait_for(args)?,
        },
        "run" => ControlCommand::Run {
            name: args.required_text("COMMAND")?,
        },
        "commands" => ControlCommand::Commands,
        "logs" => ControlCommand::Logs {
            path: args.flag("--path"),
            lines: args
                .number("--lines", 1..=crate::diagnostics::TAIL_RECORDS)?
                .unwrap_or(100),
        },
        "update" => ControlCommand::Update(settings::update(args)?),
        "diagnostics" => {
            match args.verb().as_ref().and_then(|verb| verb.to_str()) {
                Some("bundle") => {}
                _ => return Err(CliError::InvalidValue("bundle")),
            }
            ControlCommand::Diagnostics {
                output: args.required("PATH")?.into(),
            }
        }
        "integrations" => ControlCommand::Integrations(settings::integrations(args)?),
        _ => return Err(CliError::UnknownOption(family.into())),
    })
}

/// `--for 90m`: seconds, minutes, or hours.
fn duration(text: &str) -> Option<u64> {
    let (number, unit) = text.split_at(text.find(|c: char| !c.is_ascii_digit())?);
    let number: u64 = number.parse().ok()?;
    let seconds = match unit {
        "s" => number,
        "m" => number.checked_mul(60)?,
        "h" => number.checked_mul(3600)?,
        _ => return None,
    };
    (seconds > 0).then_some(seconds)
}

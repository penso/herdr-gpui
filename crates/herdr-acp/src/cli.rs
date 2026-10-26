//! Command-line parsing, pure and on OS strings so paths need not be UTF-8.

use std::{
    ffi::{OsStr, OsString},
    num::{NonZeroU32, NonZeroU64},
    path::PathBuf,
};

use crate::{agents::Agent, mirror::transcript::TranscriptAgent};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("name an agent ({}) or give a command after `--`", Agent::names())]
    MissingAgent,
    #[error("unknown agent `{0}`; known agents are {names}", names = Agent::names())]
    UnknownAgent(String),
    #[error("unknown option `{}`", .0.to_string_lossy())]
    UnknownOption(OsString),
    #[error("`{0}` needs a value")]
    MissingValue(&'static str),
    #[error("`{0}` must be valid UTF-8")]
    NotUtf8(&'static str),
    #[error("give the agent command after `--`")]
    EmptyCommand,
    #[error("`connect` needs a chat's endpoint id")]
    MissingEndpoint,
    #[error("`{0}` is not a chat endpoint id")]
    InvalidEndpoint(String),
    #[error("mirror and observe support only codex and claude transcripts")]
    TranscriptAgent,
    #[error("observe needs a Codex/Claude session UUID, positive PID and process start ticks")]
    ObserveIdentity,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Parsed {
    Run(Options),
    /// Carry a client connection to a running chat over stdin and stdout.
    Connect(herdr_chat_protocol::EndpointId),
    /// Watch an existing CLI transcript without starting or driving an agent.
    Mirror(MirrorOptions),
    /// Project one verified running CLI process over standard streams.
    Observe(ObserveOptions),
    /// List the agents installed here, one name per line.
    Agents,
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Options {
    /// Reach a built-in agent through its ACP adapter even where `herdr-acp`
    /// can speak to the agent itself.
    pub adapter: bool,
    pub agent: AgentChoice,
    /// Listen for Herdr UI and other clients.
    pub attach: bool,
    pub cwd: Option<PathBuf>,
    /// Run without the in-pane chat view.
    pub headless: bool,
    /// The agent's session to continue instead of starting a new one.
    pub load: Option<String>,
    /// The display name for a custom command.
    pub name: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AgentChoice {
    Builtin {
        agent: Agent,
        args: Vec<OsString>,
    },
    Command {
        program: OsString,
        args: Vec<OsString>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MirrorOptions {
    pub agent: TranscriptAgent,
    pub endpoint: Option<herdr_chat_protocol::EndpointId>,
    pub pane: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ObserveOptions {
    pub agent: TranscriptAgent,
    pub pane: Option<String>,
    pub pid: u32,
    pub session: String,
    /// The process's start time in clock ticks, which a reused PID lacks.
    pub started: u64,
}

pub(crate) const HELP: &str = "\
Runs an ACP agent in this terminal and shares the conversation with Herdr UI.

Usage:
  herdr-acp [OPTIONS] AGENT [-- AGENT_ARGS...]
  herdr-acp [OPTIONS] -- COMMAND [ARGS...]
  herdr-acp connect ID   Attach stdin and stdout to the running chat ID
                         (Herdr UI runs this over SSH for remote chats)
  herdr-acp agents       List the agents installed here, one per line
  herdr-acp observe codex|claude SESSION_ID PID START_TICKS [--pane ID]
                        Render the existing Linux agent over stdin/stdout.
                        Starts no agent; input stays in its original terminal.
  herdr-acp mirror codex|claude FILE [--pane ID] [--endpoint ID]
                        Watch an existing JSONL transcript, read-only; starts
                        no agent and changes no Herdr pane metadata. Prints
                        its endpoint ID once ready. Stop with Ctrl+C.

Agents:
  claude     claude itself, or with --acp claude-agent-acp
             (or npx @agentclientprotocol/claude-agent-acp)
  codex      codex app-server, or with --acp codex-acp
             (or npx @agentclientprotocol/codex-acp)
  copilot    copilot --acp
  gemini     gemini --acp
  opencode   opencode acp
  pi         pi --mode rpc, or with --acp pi-acp (or npx pi-acp)

Options:
  --acp          Use the agent's ACP adapter, not the agent itself
                 (also HERDR_ACP_ADAPTER=1)
  --cwd DIR      Run the session in DIR instead of the current directory
  --load ID      Continue the agent's session ID instead of starting a new one
  --name NAME    The name to show for a custom command
  --headless     Draw nothing; clients such as Herdr UI drive the session
  --no-attach    Do not listen for Herdr UI
  -h, --help     Show this help
  -V, --version  Show the version

Keys:
  Enter          Send the prompt, or queue it while the agent works
  Alt+Enter      New line
  Esc            Cancel the running turn
  /              Complete the agent's commands: Tab takes one, Up/Down choose
  Up/Down        Recall earlier prompts from an empty prompt
  Shift+Tab      Next session mode
  Ctrl+O         Settings: model, thinking level, mode (Left/Right change)
  PgUp/PgDn      Scroll
  Ctrl+D         Quit when the prompt is empty
";

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Parsed, Error> {
    let mut args = args.into_iter().peekable();
    // A subcommand word is consumed before its own arguments are parsed.
    match args.peek().and_then(|arg| arg.to_str()) {
        Some("agents") => {
            args.next();
            return match args.next() {
                Some(extra) => Err(Error::UnknownOption(extra)),
                None => Ok(Parsed::Agents),
            };
        }
        Some("connect") => {
            args.next();
            let id = utf8(args.next(), "connect").map_err(|_| Error::MissingEndpoint)?;
            if let Some(extra) = args.next() {
                return Err(Error::UnknownOption(extra));
            }
            return id
                .parse()
                .map(Parsed::Connect)
                .map_err(|_| Error::InvalidEndpoint(id));
        }
        Some("mirror") => {
            args.next();
            return mirror(args);
        }
        Some("observe") => {
            args.next();
            return observe(args);
        }
        _ => {}
    }
    let mut options = Options {
        adapter: false,
        agent: AgentChoice::Command {
            program: OsString::new(),
            args: Vec::new(),
        },
        attach: true,
        cwd: None,
        headless: false,
        load: None,
        name: None,
    };
    let mut agent = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Parsed::Help),
            Some("-V" | "--version") => return Ok(Parsed::Version),
            Some("--acp") => options.adapter = true,
            Some("--headless") => options.headless = true,
            Some("--no-attach") => options.attach = false,
            Some("--cwd") => {
                options.cwd = Some(args.next().ok_or(Error::MissingValue("--cwd"))?.into());
            }
            Some("--load") => options.load = Some(utf8(args.next(), "--load")?),
            Some("--name") => options.name = Some(utf8(args.next(), "--name")?),
            Some("--") => {
                let rest: Vec<OsString> = args.by_ref().collect();
                options.agent = match agent {
                    Some(agent) => AgentChoice::Builtin { agent, args: rest },
                    None => {
                        let mut rest = rest.into_iter();
                        let program = rest.next().ok_or(Error::EmptyCommand)?;
                        AgentChoice::Command {
                            program,
                            args: rest.collect(),
                        }
                    }
                };
                return Ok(Parsed::Run(options));
            }
            Some(name) if !name.starts_with('-') && agent.is_none() => {
                agent = Some(
                    name.parse::<Agent>()
                        .map_err(|_| Error::UnknownAgent(name.to_owned()))?,
                );
            }
            _ => return Err(Error::UnknownOption(arg)),
        }
    }
    let agent = agent.ok_or(Error::MissingAgent)?;
    options.agent = AgentChoice::Builtin {
        agent,
        args: Vec::new(),
    };
    Ok(Parsed::Run(options))
}

/// A subcommand's arguments, or `None` when any of them asks for help.
fn unless_help(args: impl Iterator<Item = OsString>) -> Option<std::vec::IntoIter<OsString>> {
    let args: Vec<OsString> = args.collect();
    let help = args
        .iter()
        .any(|arg| matches!(arg.to_str(), Some("-h" | "--help")));
    (!help).then(|| args.into_iter())
}

/// The agent whose native transcript `mirror` or `observe` reads.
fn transcript_agent(name: Option<&OsStr>) -> Result<TranscriptAgent, Error> {
    name.and_then(OsStr::to_str)
        .and_then(|name| name.parse::<Agent>().ok())
        .and_then(|agent| TranscriptAgent::try_from(agent).ok())
        .ok_or(Error::TranscriptAgent)
}

fn mirror(args: impl Iterator<Item = OsString>) -> Result<Parsed, Error> {
    let Some(mut args) = unless_help(args) else {
        return Ok(Parsed::Help);
    };
    let agent = transcript_agent(args.next().as_deref())?;
    let path = args.next().ok_or(Error::MissingValue("mirror FILE"))?;
    let mut options = MirrorOptions {
        agent,
        endpoint: None,
        pane: None,
        path: path.into(),
    };
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--pane") => options.pane = Some(utf8(args.next(), "--pane")?),
            Some("--endpoint") => {
                let id = utf8(args.next(), "--endpoint")?;
                options.endpoint = Some(id.parse().map_err(|_| Error::InvalidEndpoint(id))?);
            }
            _ => return Err(Error::UnknownOption(arg)),
        }
    }
    Ok(Parsed::Mirror(options))
}

fn observe(args: impl Iterator<Item = OsString>) -> Result<Parsed, Error> {
    let Some(mut args) = unless_help(args) else {
        return Ok(Parsed::Help);
    };
    let agent = transcript_agent(args.next().as_deref())?;
    let session = utf8(args.next(), "observe SESSION_ID")?;
    uuid::Uuid::parse_str(&session).map_err(|_| Error::ObserveIdentity)?;
    let pid = utf8(args.next(), "observe PID")?
        .parse::<NonZeroU32>()
        .map_err(|_| Error::ObserveIdentity)?
        .get();
    let started = utf8(args.next(), "observe START_TICKS")?
        .parse::<NonZeroU64>()
        .map_err(|_| Error::ObserveIdentity)?
        .get();
    let mut pane = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--pane") => pane = Some(utf8(args.next(), "--pane")?),
            _ => return Err(Error::UnknownOption(arg)),
        }
    }
    Ok(Parsed::Observe(ObserveOptions {
        agent,
        pane,
        pid,
        session,
        started,
    }))
}

fn utf8(value: Option<OsString>, option: &'static str) -> Result<String, Error> {
    value
        .ok_or(Error::MissingValue(option))?
        .into_string()
        .map_err(|_| Error::NotUtf8(option))
}

#[cfg(test)]
mod tests;

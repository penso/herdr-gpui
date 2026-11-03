mod args;
pub mod control;

use sheprd_client::ConnectTarget;
use std::ffi::OsString;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LaunchMode {
    #[default]
    Normal,
    Help,
    BuildInfo,
    /// Talk to the running app, or read its files, then exit without
    /// starting GPUI.
    Control(control::Invocation),
    /// Hand a `sheprd://` link to the running app, or start the app
    /// and follow it there.
    Link(crate::link::Link),
    /// Show the UI variants compiled in from `HERDR_MOCKUP_FILE`.
    #[cfg(feature = "mockup")]
    Mockup(MockupOptions),
    #[cfg(feature = "integration-test")]
    Integration,
    #[cfg(feature = "integration-test")]
    Sidebar,
    #[cfg(feature = "integration-test")]
    Performance,
}

/// `sheprd --mockup [--feedback PATH] [--capture PATH]`: never connects
/// to a daemon.
#[cfg(feature = "mockup")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MockupOptions {
    /// Where "Send to agent" writes the user's picks and notes when the running
    /// Sheprd cannot hand them to the calling pane's agent.
    pub feedback: Option<std::path::PathBuf>,
    /// Where to save a PNG of the window once it has drawn.
    pub capture: Option<std::path::PathBuf>,
}

#[derive(Debug)]
pub struct LaunchOptions {
    pub target: ConnectTarget,
    pub mode: LaunchMode,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("--socket requires a path")]
    MissingSocketPath,
    #[error("--session requires a name")]
    MissingSessionName,
    #[error("--socket may only be specified once")]
    DuplicateSocket,
    #[error("--session may only be specified once")]
    DuplicateSession,
    #[error("--session requires a UTF-8 name")]
    InvalidSessionEncoding(OsString),
    #[error("Unknown option: {}", .0.to_string_lossy())]
    UnknownOption(OsString),
    #[error("--socket cannot be combined with --session or --dev")]
    ConflictingConnectionOptions,
    #[error("browser requires a command: open, reload, feedback, skill, or --help")]
    MissingBrowserCommand,
    #[error("Unknown browser command: {}", .0.to_string_lossy())]
    UnknownBrowserCommand(OsString),
    #[error("browser open requires one URL or file")]
    MissingUrl,
    #[error("--wait requires a number of seconds")]
    InvalidWait,
    #[error("browser open accepts one URL; unexpected {}", .0.to_string_lossy())]
    UnexpectedArgument(OsString),
    #[error("--workspace requires an ID")]
    MissingWorkspace,
    #[error("browser arguments must be UTF-8")]
    InvalidBrowserEncoding(OsString),
    #[error("{0} requires a value")]
    MissingValue(&'static str),
    #[error("{0} may only be specified once")]
    DuplicateOption(&'static str),
    #[error("Invalid {0}")]
    InvalidValue(&'static str),
    #[error("{command} requires {what}")]
    MissingArgument {
        command: &'static str,
        what: &'static str,
    },
    #[error("{command}: unexpected {}", argument.to_string_lossy())]
    Unexpected {
        command: &'static str,
        argument: OsString,
    },
    #[error("Unknown {family} command: {}", command.to_string_lossy())]
    UnknownCommand {
        family: &'static str,
        command: OsString,
    },
    #[error("arguments must be UTF-8")]
    InvalidEncoding(OsString),
    #[error("invalid link: {0}")]
    Link(#[from] crate::link::Error),
    #[error("a sheprd:// link must be the only argument")]
    LinkArguments,
    #[cfg(feature = "mockup")]
    #[error("{0} requires a path")]
    MissingMockupPath(&'static str),
    #[cfg(feature = "mockup")]
    #[error("{0} may only be specified once")]
    DuplicateMockupPath(&'static str),
    #[cfg(feature = "integration-test")]
    #[error("native test modes are mutually exclusive and may only be specified once")]
    ConflictingTestModes,
    #[cfg(feature = "integration-test")]
    #[error("fixture tests cannot be combined with connection options or --integration-test")]
    ConflictingFixtureOptions,
    #[cfg(feature = "integration-test")]
    #[error("--integration-test requires an explicit --socket")]
    MissingIntegrationSocket,
    #[cfg(all(feature = "integration-test", not(any(target_os = "macos", windows))))]
    #[error("--performance-test currently requires macOS or Windows native event delivery")]
    UnsupportedPerformancePlatform,
}

// The framed record is also read without execution by cross-platform packaging.
pub fn build_info() -> &'static str {
    const PREFIX_LEN: usize = "\0HERDR_BUILD_IDENTITY_V1\n".len();
    const RECORD: &str = concat!(
        "\0HERDR_BUILD_IDENTITY_V1\n",
        "worktree=",
        env!("HERDR_BUILD_WORKTREE"),
        "\n",
        "branch=",
        env!("HERDR_BUILD_BRANCH"),
        "\n",
        "pr=",
        env!("HERDR_BUILD_PR"),
        "\n\0"
    );
    let record = std::hint::black_box(RECORD);
    &record[PREFIX_LEN..record.len() - 1]
}

/// The arguments after a leading `--mockup`. Paths stay OS strings.
#[cfg(feature = "mockup")]
fn parse_mockup(mut args: impl Iterator<Item = OsString>) -> Result<LaunchMode, CliError> {
    let mut options = MockupOptions::default();
    while let Some(arg) = args.next() {
        let (flag, slot) = match arg.to_str() {
            Some("--help" | "-h") => return Ok(LaunchMode::Help),
            Some("--feedback") => ("--feedback", &mut options.feedback),
            Some("--capture") => ("--capture", &mut options.capture),
            _ => return Err(CliError::UnknownOption(arg)),
        };
        let path = args
            .next()
            .filter(|value| !value.is_empty() && !value.as_encoded_bytes().starts_with(b"-"))
            .ok_or(CliError::MissingMockupPath(flag))?;
        if slot.replace(path.into()).is_some() {
            return Err(CliError::DuplicateMockupPath(flag));
        }
    }
    Ok(LaunchMode::Mockup(options))
}

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<Self, CliError> {
        let mut args = args.into_iter().map(Into::into).peekable();
        if let Some(family) = args.next_if(control::is_family) {
            return Ok(Self {
                target: ConnectTarget::Local,
                mode: LaunchMode::Control(control::parse(family, args)?),
            });
        }
        if let Some(link) = args.next_if(|arg| crate::link::is_link(arg)) {
            // The shell substitutes the whole link into one argument; a
            // second one means it was split, perhaps to smuggle an option.
            if args.next().is_some() {
                return Err(CliError::LinkArguments);
            }
            let link = link.into_string().map_err(CliError::InvalidEncoding)?;
            return Ok(Self {
                target: ConnectTarget::Local,
                mode: LaunchMode::Link(crate::link::Link::parse(&link)?),
            });
        }
        #[cfg(feature = "mockup")]
        if args.peek().is_some_and(|arg| arg == "--mockup") {
            args.next();
            return Ok(Self {
                target: ConnectTarget::Local,
                mode: parse_mockup(args)?,
            });
        }
        let mut socket = None;
        let mut session = None;
        let mut development = false;
        #[cfg(feature = "integration-test")]
        let mut mode = LaunchMode::Normal;
        #[cfg(not(feature = "integration-test"))]
        let mode = LaunchMode::Normal;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--help" | "-h" | "--build-info") => {
                    return Ok(Self {
                        target: ConnectTarget::Local,
                        mode: if arg == "--build-info" {
                            LaunchMode::BuildInfo
                        } else {
                            LaunchMode::Help
                        },
                    });
                }
                Some("--socket" | "--session") => {
                    let is_socket = arg == "--socket";
                    let missing = if is_socket {
                        CliError::MissingSocketPath
                    } else {
                        CliError::MissingSessionName
                    };
                    let value = args
                        .next()
                        .filter(|value| {
                            !value.is_empty() && !value.as_encoded_bytes().starts_with(b"-")
                        })
                        .ok_or(missing)?;
                    if is_socket {
                        if socket.replace(value).is_some() {
                            return Err(CliError::DuplicateSocket);
                        }
                    } else {
                        let value = value
                            .into_string()
                            .map_err(CliError::InvalidSessionEncoding)?;
                        if session.replace(value).is_some() {
                            return Err(CliError::DuplicateSession);
                        }
                    }
                }
                Some("--dev") => development = true,
                #[cfg(feature = "integration-test")]
                Some(flag @ ("--integration-test" | "--sidebar-test" | "--performance-test")) => {
                    let next = match flag {
                        "--integration-test" => LaunchMode::Integration,
                        "--sidebar-test" => LaunchMode::Sidebar,
                        _ => LaunchMode::Performance,
                    };
                    if mode != LaunchMode::Normal {
                        return Err(CliError::ConflictingTestModes);
                    }
                    mode = next;
                }
                _ => {
                    return Err(CliError::UnknownOption(arg));
                }
            }
        }
        if socket.is_some() && (session.is_some() || development) {
            return Err(CliError::ConflictingConnectionOptions);
        }
        #[cfg(feature = "integration-test")]
        {
            if matches!(mode, LaunchMode::Sidebar | LaunchMode::Performance)
                && (socket.is_some() || session.is_some() || development)
            {
                return Err(CliError::ConflictingFixtureOptions);
            }
            if mode == LaunchMode::Integration && socket.is_none() {
                return Err(CliError::MissingIntegrationSocket);
            }
            #[cfg(not(any(target_os = "macos", windows)))]
            if mode == LaunchMode::Performance {
                return Err(CliError::UnsupportedPerformancePlatform);
            }
        }
        let target = match (socket, session) {
            (Some(path), _) => ConnectTarget::Socket(path.into()),
            (_, Some(name)) => ConnectTarget::Session { name, development },
            _ if development => ConnectTarget::Session {
                name: "default".into(),
                development,
            },
            _ => ConnectTarget::Local,
        };
        Ok(Self { target, mode })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

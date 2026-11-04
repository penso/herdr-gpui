//! The command line: parsing from OS strings, so script paths need not be
//! UTF-8, into commands that `run` carries out against any writer.

use crate::report::By;
use std::{ffi::OsString, path::PathBuf};

mod help;
mod run;
mod watch;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

pub use help::HELP;
pub use run::run;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Approve {
        file: PathBuf,
        hash: Option<String>,
        json: bool,
    },
    Check {
        file: PathBuf,
        json: bool,
    },
    Disable {
        file: PathBuf,
        json: bool,
    },
    Enable {
        file: PathBuf,
        json: bool,
    },
    Help,
    InstallService {
        json: bool,
    },
    Log {
        target: LogTarget,
        json: bool,
        follow: bool,
    },
    Pause {
        target: Target,
        json: bool,
    },
    Repo {
        action: RepoAction,
        json: bool,
    },
    Resume {
        target: Target,
        json: bool,
    },
    RunNow {
        file: PathBuf,
        trigger: Option<String>,
        json: bool,
    },
    RunShow {
        id: i64,
        json: bool,
    },
    Runs {
        script: Option<PathBuf>,
        json: bool,
    },
    Serve,
    Shutdown {
        json: bool,
    },
    Start {
        json: bool,
    },
    Status {
        json: bool,
    },
    Steps {
        json: bool,
    },
    Stop {
        run: i64,
        json: bool,
    },
    UninstallService {
        json: bool,
    },
    Usage {
        script: Option<PathBuf>,
        since: Since,
        by: By,
        json: bool,
    },
    Version,
    Watch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    All,
    Script(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogTarget {
    Daemon { lines: usize },
    Run { id: i64, step: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepoAction {
    Add(PathBuf),
    List,
    Remove(PathBuf),
}

/// How far back `usage` looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Since {
    Day,
    Week,
    Month,
    All,
}

impl Since {
    pub fn start(self, now: i64) -> i64 {
        match self {
            Self::Day => now - 86_400,
            Self::Week => now - 7 * 86_400,
            Self::Month => now - 30 * 86_400,
            Self::All => 0,
        }
    }
}

/// The most daemon log lines one answer carries.
pub const MAX_LOG_LINES: usize = 5_000;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("`{flag}` does not take `{value}`")]
    BadValue { flag: &'static str, value: String },
    #[error("`{0}` is missing")]
    Missing(&'static str),
    #[error("no command given; try `sheprd-automate --help`")]
    NoCommand,
    #[error("unexpected argument `{0}`")]
    Unexpected(String),
    #[error("unknown command `{0}`; try `sheprd-automate --help`")]
    UnknownCommand(String),
}

/// One command's arguments: its positionals and the flags it may take.
#[derive(Default)]
struct Arguments {
    positional: Vec<OsString>,
    json: bool,
    all: bool,
    follow: bool,
    daemon: bool,
    hash: Option<String>,
    trigger: Option<String>,
    step: Option<String>,
    lines: Option<String>,
    since: Option<String>,
    by: Option<String>,
}

impl Arguments {
    fn read(args: impl Iterator<Item = OsString>, allowed: &[&'static str]) -> Result<Self, Error> {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let Some(flag) = arg.to_str().filter(|text| text.starts_with("--")) else {
                parsed.positional.push(arg);
                continue;
            };
            let Some(flag) = allowed.iter().copied().find(|known| *known == flag) else {
                return Err(Error::Unexpected(flag.to_owned()));
            };
            let mut value = || -> Result<String, Error> {
                args.next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or(Error::Missing(flag))
            };
            match flag {
                "--json" => parsed.json = true,
                "--all" => parsed.all = true,
                "--follow" => parsed.follow = true,
                "--daemon" => parsed.daemon = true,
                "--hash" => parsed.hash = Some(value()?),
                "--trigger" => parsed.trigger = Some(value()?),
                "--step" => parsed.step = Some(value()?),
                "--lines" => parsed.lines = Some(value()?),
                "--since" => parsed.since = Some(value()?),
                _ => parsed.by = Some(value()?),
            }
        }
        Ok(parsed)
    }

    fn extra(&self, at: usize) -> Result<(), Error> {
        match self.positional.get(at) {
            Some(extra) => Err(Error::Unexpected(extra.to_string_lossy().into_owned())),
            None => Ok(()),
        }
    }

    fn none(&self) -> Result<(), Error> {
        self.extra(0)
    }

    fn one(&mut self, name: &'static str) -> Result<OsString, Error> {
        self.extra(1)?;
        self.positional.pop().ok_or(Error::Missing(name))
    }

    fn optional(&mut self) -> Result<Option<OsString>, Error> {
        self.extra(1)?;
        Ok(self.positional.pop())
    }

    /// The positional after a subcommand word such as `runs show`.
    fn second(&mut self, name: &'static str) -> Result<OsString, Error> {
        self.extra(2)?;
        self.positional
            .pop()
            .filter(|_| self.positional.len() == 1)
            .ok_or(Error::Missing(name))
    }
}

fn number<T: std::str::FromStr>(flag: &'static str, value: &str) -> Result<T, Error> {
    value.parse().map_err(|_| Error::BadValue {
        flag,
        value: value.to_owned(),
    })
}

fn os_number<T: std::str::FromStr>(flag: &'static str, value: &OsString) -> Result<T, Error> {
    number(flag, &value.to_string_lossy())
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, Error> {
    let mut args = args.into_iter();
    let command = args.next().ok_or(Error::NoCommand)?;
    let name = command.to_string_lossy().into_owned();
    let flags: &[&'static str] = match name.as_str() {
        "approve" => &["--json", "--hash"],
        "pause" | "resume" => &["--json", "--all"],
        "run-now" => &["--json", "--trigger"],
        "log" => &["--json", "--follow", "--daemon", "--step", "--lines"],
        "usage" => &["--json", "--since", "--by"],
        "serve" | "watch" | "help" | "-h" | "--help" | "-V" | "--version" => &[],
        _ => &["--json"],
    };
    let mut parsed = Arguments::read(args, flags)?;
    let json = parsed.json;
    Ok(match name.as_str() {
        "-h" | "--help" | "help" => parsed.none().map(|()| Command::Help)?,
        "-V" | "--version" => parsed.none().map(|()| Command::Version)?,
        "approve" => Command::Approve {
            file: parsed.one("file")?.into(),
            hash: parsed.hash,
            json,
        },
        "check" => Command::Check {
            file: parsed.one("file")?.into(),
            json,
        },
        "disable" => Command::Disable {
            file: parsed.one("script")?.into(),
            json,
        },
        "enable" => Command::Enable {
            file: parsed.one("script")?.into(),
            json,
        },
        "install-service" => parsed.none().map(|()| Command::InstallService { json })?,
        "uninstall-service" => parsed.none().map(|()| Command::UninstallService { json })?,
        "log" => {
            let target = if parsed.daemon {
                parsed.none()?;
                let lines = match &parsed.lines {
                    Some(lines) => number("--lines", lines)?,
                    None => 200,
                };
                LogTarget::Daemon {
                    lines: lines.min(MAX_LOG_LINES),
                }
            } else {
                let run = parsed.one("run")?;
                LogTarget::Run {
                    id: os_number("run", &run)?,
                    step: parsed
                        .step
                        .as_deref()
                        .map(|step| number("--step", step))
                        .transpose()?,
                }
            };
            if parsed.follow && json {
                return Err(Error::Unexpected("--follow with --json".to_owned()));
            }
            Command::Log {
                target,
                json,
                follow: parsed.follow,
            }
        }
        "pause" | "resume" => {
            let target = if parsed.all {
                parsed.none()?;
                Target::All
            } else {
                Target::Script(parsed.one("script or --all")?.into())
            };
            if name == "pause" {
                Command::Pause { target, json }
            } else {
                Command::Resume { target, json }
            }
        }
        "repo" => {
            let action = parsed
                .positional
                .first()
                .map(|action| action.to_string_lossy().into_owned());
            let action = match action.as_deref() {
                Some("add") => RepoAction::Add(parsed.second("path")?.into()),
                Some("remove") => RepoAction::Remove(parsed.second("path")?.into()),
                Some("list") | None => {
                    parsed.extra(1)?;
                    RepoAction::List
                }
                Some(other) => return Err(Error::Unexpected(other.to_owned())),
            };
            Command::Repo { action, json }
        }
        "run-now" => Command::RunNow {
            file: parsed.one("script")?.into(),
            trigger: parsed.trigger,
            json,
        },
        "runs" => match parsed
            .positional
            .first()
            .map(|first| first.to_string_lossy().into_owned())
        {
            Some(show) if show == "show" => Command::RunShow {
                id: os_number("run", &parsed.second("run")?)?,
                json,
            },
            _ => Command::Runs {
                script: parsed.optional()?.map(PathBuf::from),
                json,
            },
        },
        "serve" => parsed.none().map(|()| Command::Serve)?,
        "shutdown" => parsed.none().map(|()| Command::Shutdown { json })?,
        "start" => parsed.none().map(|()| Command::Start { json })?,
        "status" | "list" => parsed.none().map(|()| Command::Status { json })?,
        "steps" => parsed.none().map(|()| Command::Steps { json })?,
        "stop" => Command::Stop {
            run: os_number("run", &parsed.one("run")?)?,
            json,
        },
        "usage" => {
            let since = match parsed.since.as_deref() {
                None | Some("24h" | "1d") => Since::Day,
                Some("7d") => Since::Week,
                Some("30d") => Since::Month,
                Some("all") => Since::All,
                Some(other) => {
                    return Err(Error::BadValue {
                        flag: "--since",
                        value: other.to_owned(),
                    });
                }
            };
            let by = match parsed.by.as_deref() {
                None | Some("script") => By::Script,
                Some("run") => By::Run,
                Some("step") => By::Step,
                Some(other) => {
                    return Err(Error::BadValue {
                        flag: "--by",
                        value: other.to_owned(),
                    });
                }
            };
            Command::Usage {
                script: parsed.optional()?.map(PathBuf::from),
                since,
                by,
                json,
            }
        }
        "watch" => parsed.none().map(|()| Command::Watch)?,
        _ => return Err(Error::UnknownCommand(name)),
    })
}

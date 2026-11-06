//! The step catalog: the only things a script can make happen. Each step is
//! declared once below; the table gives the Luau global, the summary
//! `herdr-automate steps` prints, and the typed arguments.

use crate::args::{Site, Text};
use mlua::{Lua, MultiValue, Table, Value};
use serde::Serialize;
use std::{fmt, time::Duration};

/// The `.herdr/worktree.toml` scripts `run` may start; `archive` removes
/// the worktree, so it stays behind the app's own dialog.
const WORKTREE_SCRIPTS: &[&str] = &["setup", "run"];

/// The longest a single step may be given to finish.
pub const MAX_STEP_TIMEOUT: Duration = Duration::from_secs(24 * 3_600);

macro_rules! kinds {
    ($($variant:ident => $name:literal, $usage:literal, $summary:literal;)*) => {
        /// Which step a script called.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum Kind {
            $($variant,)*
        }

        impl Kind {
            pub const ALL: &[Self] = &[$(Self::$variant,)*];

            /// The Luau global that calls this step.
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// How a script calls it.
            pub fn usage(self) -> &'static str {
                match self {
                    $(Self::$variant => $usage,)*
                }
            }

            pub fn summary(self) -> &'static str {
                match self {
                    $(Self::$variant => $summary,)*
                }
            }
        }
    };
}

kinds! {
    // keep-sorted start
    Agent => "agent", "agent { harness = \"claude\", prompt = \"...\", worktree = \"name\", base = \"main\" }", "Start an agent in a new worktree and send it the prompt";
    Notify => "notify", "notify(\"text\")", "Post a notification on the host, and in Sheprd when it is open";
    Pull => "pull", "pull(\"main\")", "Fast-forward a branch in the repository's main checkout; fails if it is dirty or has diverged";
    Run => "run", "run(\"setup\") or run(\"run\")", "Run the setup or run script from .herdr/worktree.toml and keep its output";
    Sh => "sh", "sh(\"command\")", "Run a shell command and keep its output; the plan marks it as an arbitrary command";
    Worktree => "worktree", "worktree { name = \"name\", base = \"main\" }", "Create a worktree";
    // keep-sorted end
}

/// One step a script asked for, with its arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "step")]
pub enum Step {
    Agent {
        harness: String,
        prompt: String,
        worktree: Option<String>,
        base: Option<String>,
        /// Whether the step waits for the agent to finish.
        wait: bool,
    },
    Notify {
        text: String,
    },
    Pull {
        branch: String,
    },
    Run {
        name: String,
    },
    Sh {
        command: String,
    },
    Worktree {
        name: String,
        base: Option<String>,
    },
}

impl Step {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Agent { .. } => Kind::Agent,
            Self::Notify { .. } => Kind::Notify,
            Self::Pull { .. } => Kind::Pull,
            Self::Run { .. } => Kind::Run,
            Self::Sh { .. } => Kind::Sh,
            Self::Worktree { .. } => Kind::Worktree,
        }
    }
}

/// A step and how the script wants it run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Call {
    #[serde(flatten)]
    pub step: Step,
    /// When true, a failure stops the run; when false the script gets the
    /// result and decides.
    pub check: bool,
    #[serde(rename = "timeout_seconds", serialize_with = "seconds")]
    pub timeout: Option<Duration>,
}

fn seconds<S: serde::Serializer>(
    timeout: &Option<Duration>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match timeout {
        Some(timeout) => serializer.serialize_some(&timeout.as_secs()),
        None => serializer.serialize_none(),
    }
}

impl Call {
    /// Reads a call to `kind` from the script's arguments.
    pub(crate) fn read(kind: Kind, args: MultiValue) -> mlua::Result<Self> {
        let site = Site {
            function: kind.name(),
        };
        let mut args = args.into_iter();
        let first = args.next().unwrap_or(Value::Nil);
        let second = args.next().unwrap_or(Value::Nil);
        match kind {
            Kind::Agent | Kind::Worktree => {
                let Value::Table(table) = first else {
                    return Err(site.refuse(crate::Problem::Type {
                        field: "argument",
                        expected: "a table, as in agent { ... }",
                    }));
                };
                let (check, timeout) = options(site, &table)?;
                let step = if kind == Kind::Agent {
                    site.only(
                        &table,
                        &[
                            "harness", "prompt", "worktree", "base", "wait", "check", "timeout",
                        ],
                    )?;
                    Step::Agent {
                        harness: site.required_text(&table, Text::identifier("harness"))?,
                        prompt: site.required_text(&table, Text::body("prompt", 64 * 1024))?,
                        worktree: site.optional_text(&table, Text::reference("worktree"))?,
                        base: site.optional_text(&table, Text::reference("base"))?,
                        wait: site.optional_bool(&table, "wait")?.unwrap_or(false),
                    }
                } else {
                    site.only(&table, &["name", "base", "check", "timeout"])?;
                    Step::Worktree {
                        name: site.required_text(&table, Text::reference("name"))?,
                        base: site.optional_text(&table, Text::reference("base"))?,
                    }
                };
                Ok(Self {
                    step,
                    check,
                    timeout,
                })
            }
            Kind::Notify | Kind::Pull | Kind::Run | Kind::Sh => {
                let step = match kind {
                    Kind::Notify => Step::Notify {
                        text: site.text(first, Text::body("text", 1_024))?,
                    },
                    Kind::Pull => Step::Pull {
                        branch: site.text(first, Text::reference("branch"))?,
                    },
                    Kind::Run => {
                        let name = site.text(first, Text::name("name"))?;
                        if !WORKTREE_SCRIPTS.contains(&name.as_str()) {
                            return Err(site.refuse(crate::Problem::Range {
                                field: "name",
                                range: "`setup` or `run`, a script from .herdr/worktree.toml",
                            }));
                        }
                        Step::Run { name }
                    }
                    _ => {
                        let command = site.text(first, Text::body("command", 4_096))?;
                        if !crate::tokens::shell_tokens_placeable(&command) {
                            return Err(site.refuse(crate::Problem::Shape {
                                field: "command",
                                rule: "free of comments, heredocs, `$'…'`, `$\"…\"` and `${…}`, with \
                                       every quote closed, when it uses an earlier step's output, \
                                       so the output can be quoted safely",
                            }));
                        }
                        Step::Sh { command }
                    }
                };
                let (check, timeout) = match second {
                    Value::Nil => (true, None),
                    Value::Table(table) => {
                        site.only(&table, &["check", "timeout"])?;
                        options(site, &table)?
                    }
                    _ => {
                        return Err(site.refuse(crate::Problem::Type {
                            field: "options",
                            expected: "a table such as { check = false }",
                        }));
                    }
                };
                Ok(Self {
                    step,
                    check,
                    timeout,
                })
            }
        }
    }
}

fn options(site: Site, table: &Table) -> mlua::Result<(bool, Option<Duration>)> {
    let check = site.optional_bool(table, "check")?.unwrap_or(true);
    let timeout = site.optional_duration(
        table,
        "timeout",
        Duration::from_secs(1),
        MAX_STEP_TIMEOUT,
        "between 1s and 1d",
    )?;
    Ok((check, timeout))
}

impl fmt::Display for Call {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.step {
            Step::Agent {
                harness,
                prompt,
                worktree,
                base,
                wait,
            } => {
                write!(f, "agent {harness}")?;
                match worktree {
                    Some(worktree) => write!(f, " in worktree {worktree}")?,
                    None => f.write_str(" in a new worktree")?,
                }
                if let Some(base) = base {
                    write!(f, " off {base}")?;
                }
                write!(f, ", prompt {} chars", prompt.chars().count())?;
                if *wait {
                    f.write_str(", waits for it to finish")?;
                }
            }
            Step::Notify { text } => write!(f, "notify \"{}\"", shorten(text, 60))?,
            Step::Pull { branch } => write!(f, "pull {branch}")?,
            Step::Run { name } => write!(f, "run worktree script {name}")?,
            Step::Sh { command } => write!(f, "sh `{}` (arbitrary command)", shorten(command, 60))?,
            Step::Worktree { name, base } => {
                write!(f, "worktree {name}")?;
                if let Some(base) = base {
                    write!(f, " off {base}")?;
                }
            }
        }
        if let Some(timeout) = self.timeout {
            write!(f, ", timeout {}", crate::schedule::format_duration(timeout))?;
        }
        if !self.check {
            f.write_str(" · the script handles failure")?;
        }
        Ok(())
    }
}

impl Call {
    /// The text a step's one-line summary shortens or leaves out, which a
    /// plan shows in full under it: a command, a prompt, a notification.
    pub fn full_text(&self) -> Option<(&'static str, &str)> {
        match &self.step {
            Step::Agent { prompt, .. } => Some(("prompt", prompt)),
            Step::Notify { text } if shorten(text, 60) != *text => Some(("text", text)),
            Step::Sh { command } if shorten(command, 60) != *command => Some(("command", command)),
            _ => None,
        }
    }
}

/// The first line of `text`, at most `limit` characters.
fn shorten(text: &str, limit: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= limit && line.len() == text.len() {
        return line.to_owned();
    }
    let mut short = line.chars().take(limit).collect::<String>();
    short.push('…');
    short
}

/// What the runner reports for a step. A script sees only whether it
/// succeeded; the rest of the result reaches later steps as tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict<H> {
    Succeeded,
    Failed,
    /// The runner stops the run for its own reason: a cap, a cancel, or a
    /// call the approved plan does not have.
    Halt(H),
}

impl<H> Verdict<H> {
    pub(crate) fn to_table(&self, lua: &Lua, step: usize, kind: Kind) -> mlua::Result<Table> {
        let table = lua.create_table()?;
        table.raw_set("ok", matches!(self, Self::Succeeded))?;
        table.raw_set("log_tail", crate::tokens::log_tail(step))?;
        if kind == Kind::Agent {
            table.raw_set("run_id", crate::tokens::run_id(step))?;
        }
        Ok(table)
    }
}

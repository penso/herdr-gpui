#[cfg(feature = "engine")]
use std::fmt;
use std::{io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Compile errors, runtime errors and everything the host refused while
    /// the script ran. A refusal travels through Lua as an external error
    /// and keeps the script location Lua adds; [`Error::refusal`] finds it.
    #[cfg(feature = "engine")]
    #[error("{0}")]
    Lua(#[from] mlua::Error),
    /// The sandbox could not be set up or sealed.
    #[cfg(feature = "engine")]
    #[error(transparent)]
    Luau(#[from] herdr_luau::Error),
    #[error("the script declares no caps; call herdr.caps {{ ... }} at its top level")]
    MissingCaps,
    #[error("the script declares no trigger; add herdr.at, herdr.every or herdr.cron")]
    NoTriggers,
    #[error("could not read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("a trigger has more than {limit} paths; call fewer steps with check = false")]
    TooManyPaths { limit: usize },
    #[error("the script is {size} bytes; the limit is {limit}")]
    TooLarge { size: u64, limit: u64 },
    #[error("the script has no trigger number {0}")]
    UnknownTrigger(usize),
}

#[cfg(feature = "engine")]
impl Error {
    /// What the host refused, when that is why the script stopped.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Lua(error) => error.downcast_ref::<Refusal>(),
            _ => None,
        }
    }

    /// The budget the script ran past, if it stopped for that reason.
    pub fn budget(&self) -> Option<Budget> {
        match self {
            Self::Lua(mlua::Error::MemoryError(_)) => Some(Budget::Memory),
            Self::Lua(error) if error.chain().any(is_memory_error) => Some(Budget::Memory),
            Self::Lua(error)
                if error
                    .downcast_ref::<herdr_luau::OutOfInstructions>()
                    .is_some() =>
            {
                Some(Budget::Interrupts)
            }
            _ => match self.refusal() {
                Some(Refusal::Budget(budget)) => Some(*budget),
                _ => None,
            },
        }
    }
}

#[cfg(feature = "engine")]
fn is_memory_error(error: &(dyn std::error::Error + 'static)) -> bool {
    matches!(
        error.downcast_ref::<mlua::Error>(),
        Some(mlua::Error::MemoryError(_))
    )
}

#[cfg(feature = "engine")]
/// Something the host would not do for a script.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("the script ran past its {0} budget")]
    Budget(Budget),
    #[error("herdr.caps can be called only once")]
    CapsTwice,
    #[error("invalid argument to {function}: {problem}")]
    Invalid {
        function: &'static str,
        problem: Problem,
    },
    /// Unwinds a run the runner halted; [`crate::End::Halted`] says why.
    #[error("the runner stopped the run at step {}", .0 + 1)]
    Halted(usize),
    #[error("{0}() can be called only inside a trigger")]
    StepOutsideTrigger(&'static str),
    /// Stops a run: a step failed and the script did not ask for its result.
    #[error("step {} failed", .0 + 1)]
    StepFailed(usize),
    #[error("triggers and caps can be declared only at the script's top level")]
    TriggerInsideTrigger,
}

#[cfg(feature = "engine")]
/// The limits that stop a script that runs away.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Budget {
    Interrupts,
    Memory,
    Steps,
}

#[cfg(feature = "engine")]
impl fmt::Display for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Interrupts => "instruction",
            Self::Memory => "memory",
            Self::Steps => "step",
        })
    }
}

#[cfg(feature = "engine")]
/// Why an argument was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    #[error("`{0}` contains a control character")]
    Control(&'static str),
    #[error("`{0}` contains an invisible formatting character, such as a bidi override")]
    Format(&'static str),
    #[error("invalid cron expression `{expression}`: {reason}")]
    Cron {
        expression: String,
        reason: &'static str,
    },
    #[error("invalid days `{0}`; use daily, weekdays, weekends, mon-fri or mon,wed,fri")]
    Days(String),
    #[error("invalid duration `{0}`; use forms like 90s, 30m, 6h, 1d or 1h30m")]
    Duration(String),
    #[error("`{0}` is empty")]
    Empty(&'static str),
    #[error("`{0}` is missing")]
    Missing(&'static str),
    #[error("`{field}` must be {range}")]
    Range {
        field: &'static str,
        range: &'static str,
    },
    #[error("`{field}` must be {rule}")]
    Shape {
        field: &'static str,
        rule: &'static str,
    },
    #[error("invalid time `{0}`; use HH:MM in 24-hour form")]
    Time(String),
    #[error("unknown time zone `{0}`")]
    TimeZone(String),
    #[error("`{field}` is longer than {limit} bytes")]
    TooLong { field: &'static str, limit: usize },
    #[error("`{field}` must be {expected}")]
    Type {
        field: &'static str,
        expected: &'static str,
    },
    #[error("unknown field `{0}`")]
    Unknown(String),
}

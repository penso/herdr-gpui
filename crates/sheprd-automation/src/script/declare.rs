//! `herdr.at`, `herdr.every`, `herdr.cron`, `herdr.manual` and `herdr.caps`:
//! what a script declares at its top level. Declaring is refused once the
//! load is over, so a handler cannot add triggers while it runs.

use crate::{
    Caps, Problem, Refusal,
    args::{Site, Text},
    schedule::{ClockTime, Cron, Days, Schedule, Zone},
};
use mlua::{Function, Lua, MultiValue, Value};
use serde::Serialize;
use std::{cell::RefCell, fmt, rc::Rc, time::Duration};

/// The shortest and longest interval `herdr.every` accepts.
const EVERY_MIN: Duration = Duration::from_secs(60);
const EVERY_MAX: Duration = Duration::from_secs(30 * 86_400);

/// The most triggers one script may declare.
pub const MAX_TRIGGERS: usize = 16;

#[derive(Clone, Serialize)]
pub struct Trigger {
    /// Optional; `run-now --trigger NAME` uses it.
    pub name: Option<String>,
    pub schedule: Schedule,
    pub zone: Zone,
    #[serde(skip)]
    handler: Function,
}

impl Trigger {
    pub(super) fn handler(&self) -> &Function {
        &self.handler
    }
}

impl fmt::Debug for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Trigger")
            .field("name", &self.name)
            .field("schedule", &self.schedule)
            .field("zone", &self.zone)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{name}: ")?;
        }
        write!(f, "{}", self.schedule)?;
        match self.schedule {
            Schedule::At { .. } | Schedule::Cron { .. } => write!(f, " ({})", self.zone),
            Schedule::Every { .. } | Schedule::Manual => Ok(()),
        }
    }
}

pub(super) struct Declared {
    open: bool,
    pub(super) caps: Option<Caps>,
    pub(super) triggers: Vec<Trigger>,
}

impl Default for Declared {
    fn default() -> Self {
        Self {
            open: true,
            caps: None,
            triggers: Vec::new(),
        }
    }
}

impl Declared {
    /// What is left behind once the load is over: nothing more may be added.
    pub(super) fn closed() -> Self {
        Self {
            open: false,
            ..Self::default()
        }
    }
}

/// Adds the `herdr` table to the state's globals.
pub(super) fn install(lua: &Lua, declared: &Rc<RefCell<Declared>>) -> mlua::Result<()> {
    let herdr = lua.create_table()?;
    for kind in [
        Declare::At,
        Declare::Caps,
        Declare::Cron,
        Declare::Every,
        Declare::Manual,
    ] {
        let declared = declared.clone();
        let function = lua.create_function(move |lua, args: MultiValue| {
            let mut declared = declared.borrow_mut();
            if !declared.open {
                return Err(mlua::Error::external(Refusal::TriggerInsideTrigger));
            }
            kind.apply(lua, &mut declared, args)
        })?;
        herdr.raw_set(kind.name(), function)?;
    }
    lua.globals().raw_set("herdr", herdr)
}

#[derive(Clone, Copy)]
enum Declare {
    At,
    Caps,
    Cron,
    Every,
    Manual,
}

impl Declare {
    fn name(self) -> &'static str {
        match self {
            Self::At => "at",
            Self::Caps => "caps",
            Self::Cron => "cron",
            Self::Every => "every",
            Self::Manual => "manual",
        }
    }

    fn site(self) -> Site {
        Site {
            function: match self {
                Self::At => "herdr.at",
                Self::Caps => "herdr.caps",
                Self::Cron => "herdr.cron",
                Self::Every => "herdr.every",
                Self::Manual => "herdr.manual",
            },
        }
    }

    fn apply(self, lua: &Lua, declared: &mut Declared, args: MultiValue) -> mlua::Result<()> {
        let site = self.site();
        if let Self::Caps = self {
            let Some(Value::Table(table)) = args.into_iter().next() else {
                return Err(site.refuse(Problem::Type {
                    field: "argument",
                    expected: "a table, as in herdr.caps { ... }",
                }));
            };
            if declared.caps.is_some() {
                return Err(mlua::Error::external(Refusal::CapsTwice));
            }
            declared.caps = Some(Caps::read(&table)?);
            return Ok(());
        }
        if declared.triggers.len() >= MAX_TRIGGERS {
            return Err(site.refuse(Problem::Range {
                field: "triggers",
                range: "at most 16 per script",
            }));
        }
        let mut args = args.into_iter().collect::<Vec<_>>();
        let Some(Value::Function(handler)) = args.pop() else {
            return Err(site.refuse(Problem::Type {
                field: "handler",
                expected: "a function, given last",
            }));
        };
        let (spec, options) = match (self, args.as_slice()) {
            (Self::Manual, []) => (None, None),
            (Self::Manual, [Value::Table(options)]) => (None, Some(options.clone())),
            (_, [spec]) => (Some(spec.clone()), None),
            (_, [spec, Value::Table(options)]) => (Some(spec.clone()), Some(options.clone())),
            _ => {
                return Err(site.refuse(Problem::Type {
                    field: "arguments",
                    expected: match self {
                        Self::Manual => "herdr.manual([options,] function)",
                        _ => "a schedule, then optional options, then a function",
                    },
                }));
            }
        };
        let options = match options {
            Some(options) => options,
            None => lua.create_table()?,
        };
        let allowed: &[&str] = match self {
            Self::At => &["name", "days", "tz"],
            Self::Cron => &["name", "tz"],
            Self::Caps | Self::Every | Self::Manual => &["name"],
        };
        site.only(&options, allowed)?;
        let name = site.optional_text(&options, Text::name("name"))?;
        if let Some(name) = &name
            && declared
                .triggers
                .iter()
                .any(|trigger| trigger.name.as_ref() == Some(name))
        {
            return Err(site.refuse(Problem::Range {
                field: "name",
                range: "unique within the script",
            }));
        }
        let zone = match site.optional_text(&options, Text::name("tz"))? {
            Some(tz) => Zone::named(&tz).ok_or_else(|| site.refuse(Problem::TimeZone(tz)))?,
            None => Zone::Local,
        };
        let schedule = match (self, spec) {
            (Self::At, Some(spec)) => {
                let text = site.text(spec, Text::name("time"))?;
                let time =
                    ClockTime::parse(&text).ok_or_else(|| site.refuse(Problem::Time(text)))?;
                let days = match site.optional_text(&options, Text::name("days"))? {
                    Some(days) => {
                        Days::parse(&days).ok_or_else(|| site.refuse(Problem::Days(days)))?
                    }
                    None => Days::ALL,
                };
                Schedule::At { time, days }
            }
            (Self::Cron, Some(spec)) => {
                let text = site.text(spec, Text::name("expression"))?;
                let cron = Cron::parse(&text).map_err(|reason| {
                    site.refuse(Problem::Cron {
                        expression: text.clone(),
                        reason,
                    })
                })?;
                Schedule::Cron { cron }
            }
            (Self::Every, Some(spec)) => {
                let interval =
                    site.duration(spec, "interval", EVERY_MIN, EVERY_MAX, "between 1m and 30d")?;
                Schedule::Every {
                    seconds: interval.as_secs(),
                }
            }
            _ => Schedule::Manual,
        };
        declared.triggers.push(Trigger {
            name,
            schedule,
            zone,
            handler,
        });
        Ok(())
    }
}

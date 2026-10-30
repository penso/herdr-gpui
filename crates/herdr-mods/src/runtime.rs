//! A loaded mod: its sandboxed Luau state, its `init`/`update`/`view`, and
//! the state it keeps between calls. Each call gets the whole instruction
//! budget. A `Mod` is not `Send`: the app creates it on the worker thread
//! that runs it and sends only [`Node`]s and [`Effect`]s out.

use crate::{
    Data, Effect, Error, Node, Problem, Result, Sources, State, Topic, effect, node, source,
};
use herdr_luau::{Meter, Policy};
use mlua::{Function, Lua, MultiValue, RegistryKey, Value};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, SystemTime},
};

mod host;

const MAX_LOG_LINES: usize = 200;
const MAX_LOG_LINE: usize = 1024;

/// Bounds on one mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Per call: the load, `init`, each `update`, and each `view`.
    pub luau: herdr_luau::Limits,
    /// Elements one view may return.
    pub nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            luau: herdr_luau::Limits {
                interrupts: 2_000_000,
                memory: 16 * 1024 * 1024,
            },
            nodes: 200,
        }
    }
}

/// A line a mod wrote with `herdr.log`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub at: SystemTime,
    pub text: String,
}

pub struct Mod {
    name: String,
    lua: Lua,
    meter: Meter,
    limits: Limits,
    topics: Vec<Topic>,
    update: Option<RegistryKey>,
    view: RegistryKey,
    state: RegistryKey,
    shared: Rc<Shared>,
}

impl std::fmt::Debug for Mod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mod")
            .field("name", &self.name)
            .field("topics", &self.topics)
            .finish_non_exhaustive()
    }
}

/// What the host's Lua functions and the `Mod` share.
#[derive(Default)]
struct Shared {
    /// What `herdr.now()` returns during the current call.
    now: Cell<Option<SystemTime>>,
    log: RefCell<VecDeque<LogLine>>,
    spec: RefCell<Option<host::Spec>>,
}

impl Shared {
    /// Keeps `text`, already bounded by [`log_line`], dropping the oldest
    /// line once the log is full.
    fn log(&self, text: &str) {
        let mut log = self.log.borrow_mut();
        if log.len() == MAX_LOG_LINES {
            log.pop_front();
        }
        log.push_back(LogLine {
            at: self.now.get().unwrap_or(SystemTime::UNIX_EPOCH),
            text: text.to_owned(),
        });
    }
}

/// `herdr.log`'s arguments as one line of at most [`MAX_LOG_LINE`] bytes,
/// built within that bound: Lua strings outside the mod's memory limit are
/// never copied whole, so a mod cannot make the app allocate past it.
/// Values other than strings, numbers, and booleans print as their type,
/// without running a `__tostring` that could build a large string.
pub(super) fn log_line(values: &MultiValue) -> String {
    let mut line = String::new();
    for (index, value) in values.iter().enumerate() {
        if line.len() >= MAX_LOG_LINE {
            break;
        }
        if index > 0 {
            line.push(' ');
        }
        let room = MAX_LOG_LINE.saturating_sub(line.len());
        match value {
            Value::String(text) => {
                let bytes = text.as_bytes();
                line.push_str(&String::from_utf8_lossy(&bytes[..bytes.len().min(room)]));
            }
            Value::Nil => line.push_str("nil"),
            Value::Boolean(value) => line.push_str(if *value { "true" } else { "false" }),
            Value::Integer(value) => line.push_str(&value.to_string()),
            Value::Number(value) => line.push_str(&value.to_string()),
            other => line.push_str(other.type_name()),
        }
    }
    let mut end = line.len().min(MAX_LOG_LINE);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line.truncate(end);
    line
}

impl Mod {
    /// Loads a mod and runs its `init`. Its main chunk must call
    /// `herdr.mod { ... }` once and do nothing else with lasting effect.
    pub fn load(sources: &Sources, limits: Limits, now: SystemTime) -> Result<Self> {
        let meter = Meter::default();
        let lua = herdr_luau::open(limits.luau, Policy::Interactive, &meter)?;
        let shared = Rc::new(Shared::default());
        shared.now.set(Some(now));
        host::install(&lua, sources, &shared)?;
        herdr_luau::seal(&lua)?;
        meter.reset();
        let main = sources.module("main").ok_or(Error::NoMain)?;
        herdr_luau::text(&lua, main, source::MAIN).exec()?;
        let spec = shared.spec.borrow_mut().take().ok_or(Error::NotAMod)?;
        meter.reset();
        let state = match &spec.init {
            Some(init) => lua.registry_value::<Function>(init)?.call::<Value>(())?,
            None => Value::Table(lua.create_table()?),
        };
        let state = lua.create_registry_value(state)?;
        shared.now.set(None);
        Ok(Self {
            name: sources.name().to_owned(),
            lua,
            meter,
            limits,
            topics: spec.topics,
            update: spec.update,
            view: spec.view,
            state,
            shared,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The parts of [`State`] the mod reads; run `view` again only when one
    /// of them changed.
    pub fn topics(&self) -> &[Topic] {
        &self.topics
    }

    /// Sends `msg` to `update` and returns the effects it asked for. An
    /// `update` that returns nil keeps the state it had.
    pub fn update(&mut self, msg: &Data, now: SystemTime) -> Result<Vec<Effect>> {
        let Some(update) = &self.update else {
            return Err(Error::Message {
                at: "update".to_owned(),
                problem: Problem::Missing,
            });
        };
        self.begin(now);
        let update = self.lua.registry_value::<Function>(update)?;
        let state = self.lua.registry_value::<Value>(&self.state)?;
        let msg = msg.to_lua(&self.lua)?;
        let mut results = update.call::<MultiValue>((state, msg))?;
        let next = results.pop_front().unwrap_or(Value::Nil);
        let effects = results.pop_front().unwrap_or(Value::Nil);
        let effects =
            effect::read(&effects).map_err(|(at, problem)| Error::Effect { at, problem })?;
        if !next.is_nil() {
            self.lua.replace_registry_value(&mut self.state, next)?;
        }
        self.shared.now.set(None);
        Ok(effects)
    }

    /// Runs `view` against the app's state and checks the tree it returns;
    /// `None` when it returns nil, drawing nothing.
    pub fn view(&mut self, app: &State, now: SystemTime) -> Result<Option<Node>> {
        self.begin(now);
        let view = self.lua.registry_value::<Function>(&self.view)?;
        let state = self.lua.registry_value::<Value>(&self.state)?;
        let app = app.to_lua(&self.lua, &self.topics)?;
        let tree = view.call::<Value>((state, app))?;
        self.shared.now.set(None);
        if tree.is_nil() {
            return Ok(None);
        }
        node::read(&tree, self.limits.nodes)
            .map(Some)
            .map_err(|(at, problem)| Error::Tree { at, problem })
    }

    /// The lines written with `herdr.log` since the last call.
    pub fn take_log(&mut self) -> Vec<LogLine> {
        self.shared.log.borrow_mut().drain(..).collect()
    }

    /// Instruction checks the last call used, to show a mod's cost.
    pub fn instructions(&self) -> u64 {
        self.meter.used()
    }

    fn begin(&self, now: SystemTime) {
        self.meter.reset();
        self.shared.now.set(Some(now));
    }
}

/// Seconds since the Unix epoch, as `herdr.now()` returns them.
fn unix_seconds(time: SystemTime) -> f64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64()
}

use crate::{OutOfInstructions, Result};
use mlua::{
    Lua, LuaOptions, StdLib, VmState,
    chunk::{Chunk, ChunkMode},
};
use std::{cell::Cell, rc::Rc};

/// Bounds on a state's own computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Luau's interrupt checks: one per loop iteration and call, roughly.
    pub interrupts: u64,
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            interrupts: 10_000_000,
            memory: 32 * 1024 * 1024,
        }
    }
}

/// What a state removes beyond the libraries it never opens (`os`, `io`,
/// `debug`, `coroutine`, and code loading from files).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// The same calls every time: no randomness, no output, and no way to
    /// catch a host refusal and carry on. Automations plan on this.
    Deterministic,
    /// Live scripts such as mods: they may catch errors with `pcall` and use
    /// `math.random`, but still cannot load code, reach environments, or
    /// print; the host offers its own log.
    Interactive,
}

impl Policy {
    fn removed(self) -> &'static [&'static str] {
        match self {
            Self::Deterministic => &[
                "collectgarbage",
                "gcinfo",
                "getfenv",
                "loadstring",
                "newproxy",
                "pcall",
                "print",
                "require",
                "setfenv",
                "xpcall",
            ],
            Self::Interactive => &[
                "collectgarbage",
                "gcinfo",
                "getfenv",
                "loadstring",
                "newproxy",
                "print",
                "require",
                "setfenv",
            ],
        }
    }

    fn keeps_random(self) -> bool {
        matches!(self, Self::Interactive)
    }
}

/// Counts interrupt checks; reset before each load and run so each gets the
/// whole budget.
#[derive(Clone, Debug, Default)]
pub struct Meter(Rc<Cell<u64>>);

impl Meter {
    pub fn reset(&self) {
        self.0.set(0);
    }

    /// Interrupt checks since the last reset.
    pub fn used(&self) -> u64 {
        self.0.get()
    }
}

/// A state with the safe libraries and the budget, not yet sealed: the
/// caller adds the host's globals, then calls [`seal`].
pub fn open(limits: Limits, policy: Policy, meter: &Meter) -> Result<Lua> {
    let libs =
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::BIT | StdLib::BUFFER;
    let lua = Lua::new_with(libs, LuaOptions::new())?;
    let globals = lua.globals();
    for name in policy.removed() {
        globals.raw_set(*name, mlua::Value::Nil)?;
    }
    if !policy.keeps_random() {
        let math = globals.raw_get::<mlua::Table>("math")?;
        math.raw_set("random", mlua::Value::Nil)?;
        math.raw_set("randomseed", mlua::Value::Nil)?;
    }
    lua.set_memory_limit(limits.memory)?;
    let count = meter.0.clone();
    lua.set_interrupt(move |_| {
        let used = count.get().saturating_add(1);
        count.set(used);
        if used > limits.interrupts {
            return Err(mlua::Error::external(OutOfInstructions));
        }
        Ok(VmState::Continue)
    });
    Ok(lua)
}

/// A script's source as a chunk named `name` in errors. Always text: Luau
/// bytecode is not verified when loaded, so a file that looks like
/// bytecode could escape the sandbox; in text mode it is a syntax error.
pub fn text<'a>(lua: &Lua, source: &'a str, name: &str) -> Chunk<'a> {
    lua.load(source)
        .set_mode(ChunkMode::Text)
        .set_name(format!("={name}"))
}

/// Freezes every library and the host's globals; the script's own globals
/// go to a fresh table on top.
pub fn seal(lua: &Lua) -> Result<()> {
    Ok(lua.sandbox(true)?)
}

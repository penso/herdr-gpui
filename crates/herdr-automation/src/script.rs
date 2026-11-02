//! A loaded script: its caps and triggers, read once at load, and a way to
//! run one trigger with a caller's step executor. The planner passes a fake
//! executor; the runner passes one that does the work.

use crate::{
    Caps, Error, Refusal, Result,
    step::{Call, Kind, Verdict},
    tokens,
    vm::{self, Limits},
};
use herdr_luau::Meter;
use mlua::{Function, Lua, MultiValue, Value};
use std::{cell::RefCell, fmt, rc::Rc};

mod declare;

pub use declare::Trigger;

/// The registry slot holding the current run's step dispatcher.
const DISPATCH: &str = "herdr.dispatch";

pub struct Script {
    name: String,
    lua: Lua,
    meter: Meter,
    limits: Limits,
    caps: Caps,
    triggers: Vec<Trigger>,
}

impl fmt::Debug for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Script")
            .field("name", &self.name)
            .field("caps", &self.caps)
            .field("triggers", &self.triggers)
            .finish_non_exhaustive()
    }
}

/// How a run ended without an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End<H> {
    Completed,
    /// A checked step failed; nothing after it ran.
    Stopped {
        step: usize,
    },
    /// The runner stopped the run at `step` for its own reason.
    Halted {
        step: usize,
        halt: H,
    },
}

impl Script {
    /// Loads a script from its source. Its top level runs once, declaring
    /// caps and triggers; nothing else may happen there.
    pub fn load(name: impl Into<String>, source: &str, limits: Limits) -> Result<Self> {
        let name = name.into();
        let meter = Meter::default();
        let lua = vm::open(limits, &meter)?;
        let declared = Rc::new(RefCell::new(declare::Declared::default()));
        declare::install(&lua, &declared)?;
        install_steps(&lua)?;
        vm::seal(&lua)?;
        meter.reset();
        herdr_luau::text(&lua, source, &name).exec()?;
        let declared = declared.replace(declare::Declared::closed());
        let caps = declared.caps.ok_or(Error::MissingCaps)?;
        if declared.triggers.is_empty() {
            return Err(Error::NoTriggers);
        }
        Ok(Self {
            name,
            lua,
            meter,
            limits,
            caps,
            triggers: declared.triggers,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn caps(&self) -> &Caps {
        &self.caps
    }

    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }

    /// Runs trigger `index`, handing each step to `exec` in order. A run
    /// consumes the script, so no state carries over to the next run: the
    /// runner and the planner load the source afresh each time. A checked
    /// step that does not succeed stops the run.
    pub fn run<H>(self, index: usize, mut exec: impl FnMut(&Call) -> Verdict<H>) -> Result<End<H>> {
        let trigger = self
            .triggers
            .get(index)
            .ok_or(Error::UnknownTrigger(index))?;
        let fire = self.lua.create_table()?;
        fire.raw_set("date", tokens::DATE)?;
        fire.raw_set("time", tokens::TIME)?;
        self.meter.reset();
        let steps = self.limits.steps;
        let halted = RefCell::new(None);
        let count = std::cell::Cell::new(0usize);
        let result = self.lua.scope(|scope| {
            let dispatch = scope.create_function_mut(|lua, mut args: MultiValue| {
                let index = match args.pop_front() {
                    Some(Value::Integer(index)) => usize::try_from(index).ok(),
                    Some(Value::Number(index)) if index.fract() == 0. && index >= 0. => {
                        Some(index as usize)
                    }
                    _ => None,
                };
                let kind = index
                    .and_then(|index| Kind::ALL.get(index).copied())
                    .ok_or_else(|| mlua::Error::runtime("unknown step"))?;
                let call = Call::read(kind, args)?;
                let step = count.get();
                if step >= steps {
                    return Err(mlua::Error::external(Refusal::Budget(crate::Budget::Steps)));
                }
                count.set(step + 1);
                let verdict = exec(&call);
                if let Verdict::Halt(halt) = verdict {
                    *halted.borrow_mut() = Some((step, halt));
                    return Err(mlua::Error::external(Refusal::Halted(step)));
                }
                if call.check && !matches!(verdict, Verdict::Succeeded) {
                    return Err(mlua::Error::external(Refusal::StepFailed(step)));
                }
                verdict.to_table(lua, step, kind)
            })?;
            self.lua.set_named_registry_value(DISPATCH, dispatch)?;
            let result = trigger.handler().call::<()>(fire);
            self.lua.unset_named_registry_value(DISPATCH)?;
            Ok(result)
        })?;
        if let Some((step, halt)) = halted.into_inner() {
            return Ok(End::Halted { step, halt });
        }
        match result {
            Ok(()) => Ok(End::Completed),
            Err(error) => match error.downcast_ref::<Refusal>() {
                Some(Refusal::StepFailed(step)) => Ok(End::Stopped { step: *step }),
                _ => Err(Error::Lua(error)),
            },
        }
    }
}

/// A global per step that forwards to the current run's dispatcher, or
/// refuses when no run is going on (at the script's top level).
fn install_steps(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for (index, kind) in Kind::ALL.iter().copied().enumerate() {
        let index = i64::try_from(index).map_err(mlua::Error::external)?;
        let step = lua.create_function(move |lua, mut args: MultiValue| {
            let Some(dispatch) = lua.named_registry_value::<Option<Function>>(DISPATCH)? else {
                return Err(mlua::Error::external(Refusal::StepOutsideTrigger(
                    kind.name(),
                )));
            };
            args.push_front(Value::Integer(index));
            dispatch.call::<Value>(args)
        })?;
        globals.raw_set(kind.name(), step)?;
    }
    Ok(())
}

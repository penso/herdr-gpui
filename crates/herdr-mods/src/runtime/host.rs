//! The globals a mod gets before its state is sealed: `herdr` (`mod`, `log`,
//! `now`, `run`, `timer`) and a `require` that loads only `@herdr/ui` and the
//! mod's own `.luau` files, each once.

use super::{Shared, unix_seconds};
use crate::{
    Error, Problem, Result, Sources, Topic,
    node::{Element, command_named},
};
use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Value};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

const UI: &str = "@herdr/ui";
const SPEC_KEYS: &[&str] = &["init", "update", "view", "watch"];

/// What `herdr.mod { ... }` declared.
pub(super) struct Spec {
    pub init: Option<RegistryKey>,
    pub update: Option<RegistryKey>,
    pub view: RegistryKey,
    pub topics: Vec<Topic>,
}

pub(super) fn install(lua: &Lua, sources: &Sources, shared: &Rc<Shared>) -> Result<()> {
    let herdr = lua.create_table()?;
    herdr.raw_set("mod", declare(lua, shared)?)?;
    let log = Rc::clone(shared);
    herdr.raw_set(
        "log",
        lua.create_function(move |_, values: MultiValue| {
            log.log(&super::log_line(&values));
            Ok(())
        })?,
    )?;
    let clock = Rc::clone(shared);
    herdr.raw_set(
        "now",
        lua.create_function(move |_, ()| Ok(clock.now.get().map(unix_seconds)))?,
    )?;
    herdr.raw_set(
        "run",
        lua.create_function(|lua, name: String| {
            if command_named(&name).is_none() {
                return Err(external("herdr.run", Problem::Command(name)));
            }
            let effect = lua.create_table()?;
            effect.raw_set("effect", "run")?;
            effect.raw_set("command", name)?;
            effect.set_readonly(true);
            Ok(effect)
        })?,
    )?;
    herdr.raw_set(
        "timer",
        lua.create_function(|lua, (after, msg): (f64, Value)| {
            let effect = lua.create_table()?;
            effect.raw_set("effect", "timer")?;
            effect.raw_set("after", after)?;
            effect.raw_set("msg", msg)?;
            effect.set_readonly(true);
            Ok(effect)
        })?,
    )?;
    lua.globals().raw_set("herdr", herdr)?;
    lua.globals().raw_set("require", require(lua, sources)?)?;
    Ok(())
}

fn declare(lua: &Lua, shared: &Rc<Shared>) -> Result<Function> {
    let shared = Rc::clone(shared);
    Ok(lua.create_function(move |lua, spec: Table| {
        if shared.spec.borrow().is_some() {
            return Err(external(
                "herdr.mod",
                Problem::Expected {
                    expected: "called once",
                },
            ));
        }
        for pair in spec.pairs::<Value, Value>() {
            let (key, _) = pair?;
            let known = key
                .as_string()
                .and_then(|key| key.to_str().ok())
                .is_some_and(|key| SPEC_KEYS.contains(&&*key));
            if !known {
                return Err(external("herdr.mod", Problem::Unknown(key.to_string()?)));
            }
        }
        let function = |key: &'static str| -> mlua::Result<Option<RegistryKey>> {
            match spec.raw_get::<Value>(key)? {
                Value::Nil => Ok(None),
                Value::Function(function) => Ok(Some(lua.create_registry_value(function)?)),
                _ => Err(external(
                    key,
                    Problem::Expected {
                        expected: "a function",
                    },
                )),
            }
        };
        let init = function("init")?;
        let update = function("update")?;
        let view = function("view")?.ok_or_else(|| external("view", Problem::Missing))?;
        let topics = match spec.raw_get::<Value>("watch")? {
            Value::Nil => Vec::new(),
            Value::Table(watch) => {
                let mut topics = Vec::new();
                for name in watch.sequence_values::<String>() {
                    let topic = Topic::parse(&name?)
                        .ok_or_else(|| external("watch", Problem::OneOf(Topic::NAMES)))?;
                    if !topics.contains(&topic) {
                        topics.push(topic);
                    }
                }
                topics
            }
            _ => {
                return Err(external(
                    "watch",
                    Problem::Expected {
                        expected: "a list of topics",
                    },
                ));
            }
        };
        *shared.spec.borrow_mut() = Some(Spec {
            init,
            update,
            view,
            topics,
        });
        Ok(())
    })?)
}

/// `ui.<element> { ... }` marks a props table with its element; `ui.text`
/// also takes a bare string.
fn ui(lua: &Lua) -> Result<Table> {
    let ui = lua.create_table()?;
    for element in Element::ALL {
        let name = element.name();
        let text = *element == Element::Text;
        let build = lua.create_function(move |lua, props: Value| {
            let props = match props {
                Value::Nil => lua.create_table()?,
                Value::Table(props) => props,
                Value::String(value) if text => {
                    let props = lua.create_table()?;
                    props.raw_set("text", value)?;
                    props
                }
                _ => {
                    return Err(external(
                        name,
                        Problem::Expected {
                            expected: "a table of props",
                        },
                    ));
                }
            };
            props.raw_set("type", name)?;
            Ok(props)
        })?;
        ui.raw_set(name, build)?;
    }
    ui.set_readonly(true);
    Ok(ui)
}

/// Loads `@herdr/ui` or one of the mod's modules (`require("./parts")`),
/// once each; a module that requires itself, directly or not, is refused.
fn require(lua: &Lua, sources: &Sources) -> Result<Function> {
    let ui = ui(lua)?;
    let modules: HashMap<String, String> = sources
        .modules()
        .filter(|(stem, _)| *stem != "main")
        .map(|(stem, text)| (stem.to_owned(), text.to_owned()))
        .collect();
    let loaded = lua.create_table()?;
    let loading = Rc::new(std::cell::RefCell::new(HashSet::new()));
    Ok(lua.create_function(move |lua, name: String| {
        if name == UI {
            return Ok(Value::Table(ui.clone()));
        }
        let stem = name.strip_prefix("./").unwrap_or(&name);
        let Some(text) = modules.get(stem) else {
            return Err(external(
                "require",
                Problem::Expected {
                    expected: "@herdr/ui or ./<file> naming one of the mod's .luau files",
                },
            ));
        };
        let cached = loaded.raw_get::<Value>(stem)?;
        if !cached.is_nil() {
            return Ok(cached);
        }
        if !loading.borrow_mut().insert(stem.to_owned()) {
            return Err(external(
                "require",
                Problem::Expected {
                    expected: "modules that do not require each other in a cycle",
                },
            ));
        }
        let result = herdr_luau::text(lua, text, &format!("{stem}.luau")).eval::<Value>();
        loading.borrow_mut().remove(stem);
        let value = result?;
        let value = if value.is_nil() {
            Value::Boolean(true)
        } else {
            value
        };
        loaded.raw_set(stem, value.clone())?;
        Ok(value)
    })?)
}

fn external(at: &str, problem: Problem) -> mlua::Error {
    mlua::Error::external(Error::Effect {
        at: at.to_owned(),
        problem,
    })
}

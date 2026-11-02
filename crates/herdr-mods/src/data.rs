//! Plain values that cross between a mod and the app: messages a button or
//! timer sends back to `update`. Bounded, and free of functions, userdata,
//! and cycles, so they can leave the mod's thread and come back.

use crate::Problem;
use mlua::{Lua, Value};
use std::collections::BTreeMap;

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 256;
const MAX_STRING: usize = 4 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Nil,
    Bool(bool),
    Integer(i64),
    Number(f64),
    String(String),
    List(Vec<Data>),
    Map(BTreeMap<String, Data>),
}

impl Data {
    /// Reads a Lua value, refusing what cannot cross: functions, userdata,
    /// mixed or sparse tables, and anything past the bounds.
    pub(crate) fn read(value: &Value) -> Result<Self, (String, Problem)> {
        let mut entries = 0;
        read(value, String::new(), 0, &mut entries)
    }

    pub(crate) fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        Ok(match self {
            Self::Nil => Value::Nil,
            Self::Bool(value) => Value::Boolean(*value),
            Self::Integer(value) => Value::Integer(*value),
            Self::Number(value) => Value::Number(*value),
            Self::String(value) => Value::String(lua.create_string(value)?),
            Self::List(items) => {
                let table = lua.create_table_with_capacity(items.len(), 0)?;
                for item in items {
                    table.raw_push(item.to_lua(lua)?)?;
                }
                Value::Table(table)
            }
            Self::Map(entries) => {
                let table = lua.create_table_with_capacity(0, entries.len())?;
                for (key, value) in entries {
                    table.raw_set(key.as_str(), value.to_lua(lua)?)?;
                }
                Value::Table(table)
            }
        })
    }
}

impl From<&str> for Data {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

fn read(
    value: &Value,
    at: String,
    depth: usize,
    entries: &mut usize,
) -> Result<Data, (String, Problem)> {
    *entries += 1;
    if *entries > MAX_ENTRIES {
        let limit = MAX_ENTRIES;
        return Err((at, Problem::Entries { limit }));
    }
    Ok(match value {
        Value::Nil => Data::Nil,
        Value::Boolean(value) => Data::Bool(*value),
        Value::Integer(value) => Data::Integer(*value),
        Value::Number(value) => Data::Number(*value),
        Value::String(value) => Data::String(string(value, &at, MAX_STRING)?),
        Value::Table(table) => {
            if depth == MAX_DEPTH {
                let limit = MAX_DEPTH;
                return Err((at, Problem::Depth { limit }));
            }
            let length = table.raw_len();
            let mut map = BTreeMap::new();
            let mut list = Vec::with_capacity(length.min(MAX_ENTRIES));
            for pair in table.pairs::<Value, Value>() {
                let (key, value) = pair.map_err(|_| (at.clone(), mixed()))?;
                match key {
                    Value::Integer(index) if index >= 1 && index as usize <= length => {}
                    Value::String(key) => {
                        let key = string(&key, &at, MAX_STRING)?;
                        let item = read(&value, format!("{at}.{key}"), depth + 1, entries)?;
                        map.insert(key, item);
                    }
                    _ => return Err((at, mixed())),
                }
            }
            for index in 1..=length {
                let value = table
                    .raw_get::<Value>(index)
                    .map_err(|_| (at.clone(), mixed()))?;
                list.push(read(&value, format!("{at}[{index}]"), depth + 1, entries)?);
            }
            match (list.is_empty(), map.is_empty()) {
                (_, true) => Data::List(list),
                (true, false) => Data::Map(map),
                (false, false) => return Err((at, mixed())),
            }
        }
        _ => {
            return Err((
                at,
                Problem::Expected {
                    expected: "nil, a boolean, a number, a string, or a table of those",
                },
            ));
        }
    })
}

fn mixed() -> Problem {
    Problem::Expected {
        expected: "a list or a table with string keys, not both",
    }
}

pub(crate) fn string(
    value: &mlua::LuaString,
    at: &str,
    limit: usize,
) -> Result<String, (String, Problem)> {
    let text = value.to_str().map_err(|_| {
        (
            at.to_owned(),
            Problem::Expected {
                expected: "UTF-8 text",
            },
        )
    })?;
    if text.len() > limit {
        return Err((at.to_owned(), Problem::TooLong { limit }));
    }
    Ok(text.to_owned())
}

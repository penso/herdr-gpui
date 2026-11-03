//! What `update` asks the app to do. `herdr.run` and `herdr.timer` build
//! these as plain tables; they leave the mod as data and are checked here.

use crate::{Data, Problem, node::command_named};
use herdr_commands::Command;
use mlua::Value;
use std::time::Duration;

const MAX_EFFECTS: usize = 16;
/// Timers fire no faster than this, so a mod cannot spin the app.
pub(crate) const MIN_TIMER: Duration = Duration::from_secs(1);
const MAX_TIMER: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Runs a catalog command, as the palette would.
    Run(Command),
    /// Sends `msg` to `update` once, after `after`.
    Timer { after: Duration, msg: Data },
}

type Found<T> = Result<T, (String, Problem)>;

/// Reads `update`'s second return value: nil or a list of effects.
pub(crate) fn read(value: &Value) -> Found<Vec<Effect>> {
    let table = match value {
        Value::Nil => return Ok(Vec::new()),
        Value::Table(table) => table,
        _ => return Err(expected("effects", "a list of herdr.run or herdr.timer")),
    };
    let length = table.raw_len();
    if length > MAX_EFFECTS {
        let limit = MAX_EFFECTS;
        return Err(("effects".to_owned(), Problem::Entries { limit }));
    }
    (1..=length)
        .map(|index| {
            let at = format!("effects[{index}]");
            let effect = table
                .raw_get::<Value>(index)
                .map_err(|_| expected(&at, "an effect"))?;
            one(&effect, &at)
        })
        .collect()
}

fn one(value: &Value, at: &str) -> Found<Effect> {
    let data = Data::read(value).map_err(|(inner, problem)| (format!("{at}{inner}"), problem))?;
    let Data::Map(mut map) = data else {
        return Err(expected(at, "herdr.run(...) or herdr.timer(...)"));
    };
    let kind = map.remove("effect");
    let effect = match kind {
        Some(Data::String(kind)) if kind == "run" => {
            let Some(Data::String(name)) = map.remove("command") else {
                return Err((format!("{at}.command"), Problem::Missing));
            };
            Effect::Run(
                command_named(&name)
                    .ok_or_else(|| (format!("{at}.command"), Problem::Command(name)))?,
            )
        }
        Some(Data::String(kind)) if kind == "timer" => {
            let seconds = match map.remove("after") {
                Some(Data::Integer(seconds)) => seconds as f64,
                Some(Data::Number(seconds)) => seconds,
                _ => return Err((format!("{at}.after"), Problem::Missing)),
            };
            let after = Duration::try_from_secs_f64(seconds)
                .ok()
                .filter(|after| (MIN_TIMER..=MAX_TIMER).contains(after))
                .ok_or_else(|| expected(&format!("{at}.after"), "from 1 to 86400 seconds"))?;
            let msg = map.remove("msg").unwrap_or(Data::Nil);
            Effect::Timer { after, msg }
        }
        _ => return Err(expected(at, "herdr.run(...) or herdr.timer(...)")),
    };
    if let Some(key) = map.into_keys().next() {
        return Err((at.to_owned(), Problem::Unknown(key)));
    }
    Ok(effect)
}

fn expected(at: &str, expected: &'static str) -> (String, Problem) {
    (at.to_owned(), Problem::Expected { expected })
}

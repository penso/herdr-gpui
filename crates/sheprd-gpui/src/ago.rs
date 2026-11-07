//! How long ago something happened, in the largest whole unit: "just now",
//! "5 min ago", "yesterday", "3 days ago". Checkpoints, paired phones, and
//! the command line's checkpoint list read it from seconds elapsed.
// i18n: migrated

use crate::i18n::{self, FluentValue};

/// The message and its count for `seconds` elapsed. A clock that moved
/// back reads as now rather than in the future, so callers saturate.
fn message(seconds: u64) -> (&'static str, Option<u64>) {
    match seconds {
        0..60 => ("ago-now", None),
        60..3600 => ("ago-minutes", Some(seconds / 60)),
        3600..86_400 => ("ago-hours", Some(seconds / 3600)),
        _ => ("ago-days", Some(seconds / 86_400)),
    }
}

fn args(count: Option<u64>) -> Vec<(&'static str, FluentValue<'static>)> {
    count
        .map(|count| ("count", FluentValue::from(count)))
        .into_iter()
        .collect()
}

/// `seconds` ago in the current locale.
pub(crate) fn text(seconds: u64) -> String {
    let (id, count) = message(seconds);
    i18n::message(id, &args(count))
}

/// `seconds` ago in English, for command line output.
pub(crate) fn english(seconds: u64) -> String {
    let (id, count) = message(seconds);
    i18n::english(id, &args(count))
}

/// Seconds from `then` to `now`, both Unix seconds, never negative.
pub(crate) fn since(then: i64, now: i64) -> u64 {
    u64::try_from(now.saturating_sub(then)).unwrap_or(0)
}

#[cfg(test)]
mod tests;

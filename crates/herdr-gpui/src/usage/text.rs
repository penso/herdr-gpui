//! `herdr-usage`'s typed values in the shown language: limit kinds, times
//! left, pace, balances, and the words a report names. That crate's own
//! English (`Kind::title`, `Pace::describe`, `countdown`) stays for logs and
//! tests; what a service names itself, such as a model's window, is shown as
//! it reports it.

use super::model::{Balance, Count, Kind, Pace, Unit, Window as Limit, group};
use crate::i18n::{self, FluentValue, fl};
use herdr_usage::{Arg, Text};
use std::time::{Duration, SystemTime};

/// Words a report names in the shown language: a message `herdr-usage`
/// names by ID, with its arguments, or the service's own words.
pub(crate) fn words(text: &Text) -> String {
    write(text, i18n::message)
}

/// The same words in English, for command line output.
pub(crate) fn english(text: &Text) -> String {
    write(text, i18n::english)
}

fn write(text: &Text, message: fn(&str, &[(&str, FluentValue<'_>)]) -> String) -> String {
    match text {
        Text::Service(text) => text.clone(),
        Text::Message { id, args } => {
            let args: Vec<_> = args
                .iter()
                .map(|(name, arg)| {
                    let value = match arg {
                        Arg::Number(number) => FluentValue::from(*number),
                        Arg::Text(text) => FluentValue::from(write(text, message)),
                    };
                    (*name, value)
                })
                .collect();
            message(id, &args)
        }
    }
}

/// A window's title in English, for command line output: the fixed kinds
/// as `Kind::title` names them, and a named window's words, with any
/// arguments, rather than its message ID.
pub(crate) fn english_title(kind: &Kind) -> String {
    match kind {
        Kind::Named(name) => english(name),
        kind => kind.title(),
    }
}

/// The selector value usage.ftl's messages pick a kind by; a named window
/// takes `$name` instead.
fn kind(kind: &Kind) -> (&'static str, String) {
    match kind {
        Kind::Session => ("session", String::new()),
        Kind::Daily => ("daily", String::new()),
        Kind::Weekly => ("weekly", String::new()),
        Kind::Monthly => ("monthly", String::new()),
        Kind::Named(name) => ("named", words(name)),
    }
}

/// `Weekly`, or the service's own name for the window.
pub(super) fn title(limit: &Kind) -> String {
    let (kind, name) = kind(limit);
    fl!("usage-kind", kind = kind, name = name)
}

/// `Weekly 89% left`.
pub(super) fn left_heading(limit: &Limit) -> String {
    let (kind, name) = kind(&limit.kind);
    fl!(
        "usage-limit-left",
        kind = kind,
        name = name,
        left = limit.left()
    )
}

/// `89% left`.
pub(super) fn left(limit: &Limit) -> String {
    fl!("usage-left", left = limit.left())
}

/// `47m`, `2h 53m`, `4d 11h`: the coarsest two units that still say when,
/// as `herdr_usage::model::countdown` counts them.
pub(super) fn countdown(left: Duration) -> String {
    let minutes = left.as_secs().div_ceil(60);
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => fl!("usage-countdown-minutes", minutes = minutes),
        (0, _) => fl!("usage-countdown-hours", hours = hours, minutes = minutes),
        _ => fl!("usage-countdown-days", days = days, hours = hours),
    }
}

/// `Resets in 2h 53m`.
pub(super) fn resets_in(left: Duration) -> String {
    fl!("usage-resets-in", time = countdown(left))
}

/// `Updated 5m ago`, rounding to whole minutes.
pub(super) fn updated(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        0..60 => fl!("usage-updated-just-now"),
        seconds => fl!(
            "usage-updated-ago",
            time = countdown(Duration::from_secs(seconds - seconds % 60))
        ),
    }
}

/// `11% in reserve · Lasts until reset`, as CodexBar puts it.
pub(super) fn pace(pace: &Pace, used: f32) -> String {
    if used >= 100. {
        // The header already shows when the window resets.
        return fl!("usage-pace-ran-out");
    }
    let margin = pace.expected - used;
    let standing = if margin >= 0.5 {
        "reserve"
    } else if margin <= -0.5 {
        "deficit"
    } else {
        "even"
    };
    let (outlook, time) = match pace.runs_out {
        Some(left) => ("runs-out", countdown(left)),
        None => ("lasts", String::new()),
    };
    fl!(
        "usage-pace",
        standing = standing,
        margin = margin.abs().round(),
        outlook = outlook,
        time = time
    )
}

/// The status bar's `3% used 2h 53m`: the time left names a session or
/// weekly window, and a model window is named instead, as the service does.
pub(super) fn bar_label(limit: &Limit, now: SystemTime) -> String {
    let percent = limit.percent();
    match (&limit.kind, limit.resets_in(now)) {
        (Kind::Named(name), _) => {
            fl!("usage-bar-named", percent = percent, name = words(name))
        }
        (_, Some(left)) => fl!(
            "usage-bar-resets",
            percent = percent,
            time = countdown(left)
        ),
        (other, None) => fl!("usage-bar-kind", percent = percent, kind = kind(other).0),
    }
}

/// `$12.30`, or `$12.30 of $50.00` when the balance has a total. Amounts
/// and currencies are the service's.
pub(super) fn balance(balance: &Balance) -> String {
    let amount = amount(balance.amount, &balance.unit);
    match balance.total {
        Some(total) => fl!(
            "usage-balance-of",
            amount = amount,
            total = self::amount(total, &balance.unit)
        ),
        None => amount,
    }
}

/// `$12.30`, `12.30 EUR`, or `1,250 credits`: a currency as the crate
/// writes it, and a count with its unit, in the plural its number takes.
fn amount(amount: f64, unit: &Unit) -> String {
    let count = match unit {
        Unit::Currency(_) => {
            return Balance::new(Text::service(""), amount, unit.clone()).amount_text();
        }
        Unit::Count(count) => count,
    };
    let whole = amount.round();
    let number = if (amount - whole).abs() < 0.005 {
        group(whole as i64)
    } else {
        format!("{amount:.2}")
    };
    match count {
        Count::Credits => fl!("usage-amount-credits", amount = number, count = amount),
        Count::Points => fl!("usage-amount-points", amount = number, count = amount),
        Count::Reviews => fl!("usage-amount-reviews", amount = number, count = amount),
        Count::Named(unit) => fl!("usage-amount-named", amount = number, unit = unit.as_str()),
    }
}

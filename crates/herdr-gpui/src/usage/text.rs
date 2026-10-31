//! `herdr-usage`'s typed values in the shown language: limit kinds, times
//! left, pace, and balances. That crate's own English (`Kind::title`,
//! `Pace::describe`, `countdown`) stays for logs and tests; what a service
//! names itself, such as a model's window, is shown as it reports it.

use super::model::{Balance, Kind, Pace, Window as Limit};
use crate::i18n::fl;
use std::time::{Duration, SystemTime};

/// The selector value usage.ftl's messages pick a kind by; a named window
/// takes `$name` instead.
fn kind(kind: &Kind) -> (&'static str, &str) {
    match kind {
        Kind::Session => ("session", ""),
        Kind::Daily => ("daily", ""),
        Kind::Weekly => ("weekly", ""),
        Kind::Monthly => ("monthly", ""),
        Kind::Named(name) => ("named", name),
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
            fl!("usage-bar-named", percent = percent, name = name.as_str())
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
/// and units are the service's.
pub(super) fn balance(balance: &Balance) -> String {
    let amount = balance.amount_text();
    match balance.total {
        Some(total) => {
            let total = Balance {
                label: String::new(),
                amount: total,
                unit: balance.unit.clone(),
                total: None,
            };
            fl!(
                "usage-balance-of",
                amount = amount,
                total = total.amount_text()
            )
        }
        None => amount,
    }
}

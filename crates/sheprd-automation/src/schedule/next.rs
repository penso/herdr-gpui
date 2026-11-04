//! When a schedule fires next, and whether one is due. Pure functions of
//! explicit times, so the runner's clock never leaks into scripts and tests
//! need no clock either.

use super::{Cron, Schedule, Zone};
use jiff::{Timestamp, ToSpan, Zoned, tz::TimeZone};
use std::time::Duration;

/// A firing more than this far behind `now` counts as late.
pub const LATE_AFTER: Duration = Duration::from_secs(120);

/// The most occurrences [`due`] walks to find the latest missed one.
const MISSED_WALK: usize = 10_000;
/// The most times [`due`] restarts its walk nearer to now.
const MISSED_PASSES: usize = 32;

/// A firing the runner should start now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Due {
    /// The occurrence it stands for; with the script and trigger, the
    /// firing's key, so it runs at most once.
    pub scheduled: Timestamp,
    pub late: bool,
}

impl Zone {
    /// The zone's rules; an unknown name (already refused at load) reads as UTC.
    pub fn time_zone(&self) -> TimeZone {
        match self {
            Self::Local => TimeZone::system(),
            Self::Named(name) => TimeZone::get(name).unwrap_or(TimeZone::UTC),
        }
    }
}

impl Schedule {
    /// The first occurrence strictly after `after`. `anchor` is the last
    /// firing, which sets the phase of `every`; without one, an interval
    /// starts at `after`.
    pub fn next_after(
        &self,
        zone: &TimeZone,
        after: Timestamp,
        anchor: Option<Timestamp>,
    ) -> Option<Timestamp> {
        match self {
            Self::Manual => None,
            Self::Every { seconds } => {
                let step = i64::try_from(*seconds).ok().filter(|step| *step > 0)?;
                let anchor = anchor.unwrap_or(after);
                let behind = after.as_second().saturating_sub(anchor.as_second());
                let steps = if behind < 0 { 1 } else { behind / step + 1 };
                anchor.checked_add(steps.checked_mul(step)?.seconds()).ok()
            }
            Self::At { time, days } => {
                let start = after.to_zoned(zone.clone()).date();
                (0..=7).find_map(|offset| {
                    let date = start.checked_add(offset.days()).ok()?;
                    let weekday = u8::try_from(date.weekday().to_monday_zero_offset()).ok()?;
                    if !days.contains(weekday) {
                        return None;
                    }
                    let at = date
                        .at(
                            i8::try_from(time.hour).ok()?,
                            i8::try_from(time.minute).ok()?,
                            0,
                            0,
                        )
                        .to_zoned(zone.clone())
                        .ok()?
                        .timestamp();
                    (at > after).then_some(at)
                })
            }
            Self::Cron { cron } => next_cron(cron, zone, after),
        }
    }
}

/// Walks forward from the minute after `after`, skipping whole days and
/// hours that cannot match. Gives up after a year and a day.
fn next_cron(cron: &Cron, zone: &TimeZone, after: Timestamp) -> Option<Timestamp> {
    let limit = after.checked_add(8_808.hours()).ok()?;
    let start = after.checked_add(60.seconds()).ok()?;
    let mut at =
        Timestamp::from_second(start.as_second() - start.as_second().rem_euclid(60)).ok()?;
    while at <= limit {
        let local = at.to_zoned(zone.clone());
        let (minute, hour) = (
            u8::try_from(local.minute()).ok()?,
            u8::try_from(local.hour()).ok()?,
        );
        let (day, month) = (
            u8::try_from(local.day()).ok()?,
            u8::try_from(local.month()).ok()?,
        );
        let weekday = u8::try_from(local.weekday().to_sunday_zero_offset()).ok()?;
        at = if !cron.matches_day(day, month, weekday) {
            start_of_next(&local, Unit::Day)?
        } else if !cron.matches_hour(hour) {
            start_of_next(&local, Unit::Hour)?
        } else if !cron.matches_minute(minute) {
            at.checked_add(60.seconds()).ok()?
        } else {
            return Some(at);
        };
    }
    None
}

enum Unit {
    Day,
    Hour,
}

fn start_of_next(local: &Zoned, unit: Unit) -> Option<Timestamp> {
    let next = match unit {
        Unit::Day => local.start_of_day().ok()?.checked_add(1.day()).ok()?,
        Unit::Hour => local
            .with()
            .minute(0)
            .second(0)
            .subsec_nanosecond(0)
            .build()
            .ok()?
            .checked_add(1.hour())
            .ok()?,
    };
    Some(next.timestamp())
}

/// Whether a firing is due at `now`, given that everything up to `checked`
/// was already considered. Missed occurrences collapse into the latest one,
/// so a trigger never fires once per missed interval.
pub fn due(
    schedule: &Schedule,
    zone: &TimeZone,
    checked: Timestamp,
    anchor: Option<Timestamp>,
    now: Timestamp,
) -> Option<Due> {
    let mut latest = None;
    let mut after = checked;
    let mut anchor = anchor;
    // Each pass walks at most MISSED_WALK occurrences. One that runs out
    // short of `now` restarts nearer to it, halving the stretch it has left,
    // so the firing is the latest missed one, not one days old.
    for _ in 0..MISSED_PASSES {
        let start = after;
        let mut walked = 0;
        while walked < MISSED_WALK {
            let Some(next) = schedule.next_after(zone, after, anchor) else {
                break;
            };
            if next > now {
                break;
            }
            latest = Some(next);
            after = next;
            anchor = Some(next);
            walked += 1;
        }
        if walked < MISSED_WALK {
            break;
        }
        let left = now.as_second().saturating_sub(after.as_second());
        let span = after.as_second().saturating_sub(start.as_second());
        let skip = left.saturating_sub(span / 2).max(0);
        after = after.checked_add(skip.seconds()).unwrap_or(after);
    }
    let scheduled = latest?;
    let behind = now.as_second().saturating_sub(scheduled.as_second());
    let late = u64::try_from(behind).unwrap_or(0) > LATE_AFTER.as_secs();
    Some(Due { scheduled, late })
}

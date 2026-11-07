//! Five-field cron expressions: minute, hour, day of month, month, day of
//! week. Each field is a set of bits, so matching a minute is a few masks.

use serde::{Serialize, Serializer};
use std::fmt;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cron {
    text: String,
    minutes: u64,
    hours: u64,
    days_of_month: u64,
    months: u64,
    /// Bit 0 is Sunday, as in cron; 7 also means Sunday.
    days_of_week: u64,
    /// Cron fires when either day field matches, if both are restricted.
    any_day_of_month: bool,
    any_day_of_week: bool,
}

impl Cron {
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let fields = text.split_whitespace().collect::<Vec<_>>();
        let [minute, hour, day, month, weekday] = fields.as_slice() else {
            return Err("expected five fields: minute hour day-of-month month day-of-week");
        };
        let mut days_of_week = field(weekday, 0, 7, &WEEKDAYS)?;
        if days_of_week & (1 << 7) != 0 {
            days_of_week = (days_of_week & !(1 << 7)) | 1;
        }
        Ok(Self {
            text: fields.join(" "),
            minutes: field(minute, 0, 59, &[])?,
            hours: field(hour, 0, 23, &[])?,
            days_of_month: field(day, 1, 31, &[])?,
            months: field(month, 1, 12, &MONTHS)?,
            days_of_week,
            any_day_of_month: *day == "*",
            any_day_of_week: *weekday == "*",
        })
    }

    /// Whether a local time matches. `weekday` counts from Sunday as 0;
    /// `month` and `day` count from 1.
    pub fn matches(&self, minute: u8, hour: u8, day: u8, month: u8, weekday: u8) -> bool {
        self.matches_minute(minute)
            && self.matches_hour(hour)
            && self.matches_day(day, month, weekday)
    }

    pub(crate) fn matches_minute(&self, minute: u8) -> bool {
        has(self.minutes, minute)
    }

    pub(crate) fn matches_hour(&self, hour: u8) -> bool {
        has(self.hours, hour)
    }

    /// The month, and either day field as cron reads them.
    pub(crate) fn matches_day(&self, day: u8, month: u8, weekday: u8) -> bool {
        let day_of_month = has(self.days_of_month, day);
        let day_of_week = has(self.days_of_week, weekday);
        let day_matches = match (self.any_day_of_month, self.any_day_of_week) {
            (true, true) => true,
            (true, false) => day_of_week,
            (false, true) => day_of_month,
            (false, false) => day_of_month || day_of_week,
        };
        has(self.months, month) && day_matches
    }
}

fn has(set: u64, value: u8) -> bool {
    value < 64 && set & (1 << value) != 0
}

impl fmt::Display for Cron {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for Cron {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

/// One field: `*`, `5`, `1-5`, `*/15`, `10-50/10`, names, and lists of them.
fn field(text: &str, min: u8, max: u8, names: &[&str]) -> Result<u64, &'static str> {
    let mut set = 0u64;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step = step.parse::<u8>().map_err(|_| "a step must be a number")?;
                if step == 0 {
                    return Err("a step must be at least 1");
                }
                (range, step)
            }
            None => (part, 1),
        };
        let (first, last) = if range == "*" {
            (min, max)
        } else if let Some((first, last)) = range.split_once('-') {
            (value(first, min, names)?, value(last, min, names)?)
        } else {
            let first = value(range, min, names)?;
            (first, if step > 1 { max } else { first })
        };
        if first < min || last > max || first > last {
            return Err("a value is out of range");
        }
        for bit in (first..=last).step_by(usize::from(step)) {
            set |= 1 << bit;
        }
    }
    Ok(set)
}

fn value(text: &str, offset: u8, names: &[&str]) -> Result<u8, &'static str> {
    if let Ok(number) = text.parse::<u8>() {
        return Ok(number);
    }
    let lower = text.to_ascii_lowercase();
    names
        .iter()
        .position(|name| *name == lower)
        .and_then(|index| u8::try_from(index).ok())
        .map(|index| index + offset)
        .ok_or("a value is not a number or a known name")
}

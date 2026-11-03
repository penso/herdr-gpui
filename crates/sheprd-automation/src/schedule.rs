//! When a trigger fires: a time of day on some days, an interval, a cron
//! expression, or only when someone starts it. Every trigger can also be
//! started by hand; `Manual` is the one with no schedule at all.

use serde::Serialize;
use std::{fmt, time::Duration};

mod cron;
mod next;

pub use cron::Cron;
pub use next::{Due, LATE_AFTER, due};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Schedule {
    At { time: ClockTime, days: Days },
    Cron { cron: Cron },
    Every { seconds: u64 },
    Manual,
}

impl fmt::Display for Schedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::At { time, days } => write!(f, "{days} at {time}"),
            Self::Cron { cron } => write!(f, "cron {cron}"),
            Self::Every { seconds } => write!(
                f,
                "every {}",
                format_duration(Duration::from_secs(*seconds))
            ),
            Self::Manual => f.write_str("manual only"),
        }
    }
}

/// The zone a schedule's times are read in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "name")]
pub enum Zone {
    /// The host's own zone, whatever it is when the trigger fires.
    Local,
    Named(String),
}

impl Zone {
    /// A zone name the time zone database knows.
    pub fn named(name: &str) -> Option<Self> {
        jiff::tz::TimeZone::get(name)
            .is_ok()
            .then(|| Self::Named(name.to_owned()))
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => f.write_str("host time"),
            Self::Named(name) => f.write_str(name),
        }
    }
}

/// A time of day, to the minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ClockTime {
    pub hour: u8,
    pub minute: u8,
}

impl ClockTime {
    /// `HH:MM` in 24-hour form.
    pub fn parse(text: &str) -> Option<Self> {
        let (hour, minute) = text.split_once(':')?;
        if hour.len() != 2 || minute.len() != 2 {
            return None;
        }
        let hour = hour.parse::<u8>().ok().filter(|hour| *hour < 24)?;
        let minute = minute.parse::<u8>().ok().filter(|minute| *minute < 60)?;
        Some(Self { hour, minute })
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

/// Days of the week as a set: bit 0 is Monday.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Days(u8);

const DAY_NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

impl Days {
    pub const ALL: Self = Self(0b111_1111);
    pub const WEEKDAYS: Self = Self(0b001_1111);
    pub const WEEKENDS: Self = Self(0b110_0000);

    /// `daily`, `weekdays`, `weekends`, a range `mon-fri`, a list
    /// `mon,wed,fri`, or a list of ranges.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "daily" => return Some(Self::ALL),
            "weekdays" => return Some(Self::WEEKDAYS),
            "weekends" => return Some(Self::WEEKENDS),
            _ => {}
        }
        let mut days = 0u8;
        for part in text.split(',') {
            let part = part.trim().to_ascii_lowercase();
            let (first, last) = match part.split_once('-') {
                Some((first, last)) => (day_index(first)?, day_index(last)?),
                None => (day_index(&part)?, day_index(&part)?),
            };
            if first > last {
                return None;
            }
            for day in first..=last {
                days |= 1 << day;
            }
        }
        (days != 0).then_some(Self(days))
    }

    /// Whether `day` (0 is Monday) is in the set.
    pub fn contains(self, day: u8) -> bool {
        day < 7 && self.0 & (1 << day) != 0
    }
}

fn day_index(name: &str) -> Option<u8> {
    DAY_NAMES
        .iter()
        .position(|day| *day == name.trim())
        .and_then(|day| u8::try_from(day).ok())
}

impl fmt::Display for Days {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ALL => return f.write_str("daily"),
            Self::WEEKDAYS => return f.write_str("weekdays"),
            Self::WEEKENDS => return f.write_str("weekends"),
            _ => {}
        }
        let names = (0..7)
            .filter(|day| self.contains(*day))
            .filter_map(|day| DAY_NAMES.get(usize::from(day)).copied())
            .collect::<Vec<_>>();
        f.write_str(&names.join(","))
    }
}

/// `90s`, `30m`, `6h`, `1d`, or parts together such as `1h30m`.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut total = 0u64;
    let mut number = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        let unit = match c {
            's' => 1,
            'm' => 60,
            'h' => 3_600,
            'd' => 86_400,
            _ => return None,
        };
        let value = number.parse::<u64>().ok()?;
        number.clear();
        total = total.checked_add(value.checked_mul(unit)?)?;
    }
    (number.is_empty() && total > 0).then(|| Duration::from_secs(total))
}

/// The shortest form [`parse_duration`] reads back.
pub fn format_duration(duration: Duration) -> String {
    let mut seconds = duration.as_secs();
    let mut text = String::new();
    for (unit, suffix) in [(86_400, 'd'), (3_600, 'h'), (60, 'm'), (1, 's')] {
        if seconds >= unit {
            text.push_str(&format!("{}{suffix}", seconds / unit));
            seconds %= unit;
        }
    }
    if text.is_empty() {
        text.push_str("0s");
    }
    text
}

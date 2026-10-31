//! When a snooze can end, in the user's own time: an hour or three from
//! now, this evening, tomorrow morning, next Monday morning. These are the
//! times T3 Code offers.

use crate::i18n::fl;
use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Weekday};
use std::time::SystemTime;

const EVENING_HOUR: u32 = 18;
const MORNING_HOUR: u32 = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Preset {
    /// The message naming it, such as `overview-preset-tomorrow`.
    pub label: &'static str,
    /// When it ends, as the menu shows it: `18:00`, `Mon 09:00`.
    pub when: String,
    pub until: SystemTime,
}

/// `day`'s `hour` o'clock, or None when that time does not exist there.
fn at<Tz: TimeZone>(day: &DateTime<Tz>, hour: u32) -> Option<DateTime<Tz>> {
    let time = NaiveTime::from_hms_opt(hour, 0, 0)?;
    day.timezone()
        .from_local_datetime(&day.date_naive().and_time(time))
        .earliest()
}

/// The presets offered at `now`. This evening is left out once it is less
/// than an hour away, and next week when it is tomorrow.
pub(crate) fn presets<Tz: TimeZone>(now: &DateTime<Tz>) -> Vec<Preset>
where
    Tz::Offset: std::fmt::Display,
{
    let clock = |time: &DateTime<Tz>| time.format("%H:%M").to_string();
    let mut presets = Vec::new();
    let mut push = |label, when: String, time: DateTime<Tz>| {
        presets.push(Preset {
            label,
            when,
            until: SystemTime::from(time.with_timezone(&chrono::Utc)),
        });
    };
    for (label, hours) in [("overview-preset-hour", 1), ("overview-preset-hours", 3)] {
        let time = now.clone() + Duration::hours(hours);
        push(label, clock(&time), time);
    }
    if let Some(evening) =
        at(now, EVENING_HOUR).filter(|evening| evening.timestamp() - now.timestamp() > 3600)
    {
        push("overview-preset-evening", clock(&evening), evening);
    }
    let tomorrow = at(&(now.clone() + Duration::days(1)), MORNING_HOUR);
    if let Some(tomorrow) = tomorrow.clone() {
        push("overview-preset-tomorrow", clock(&tomorrow), tomorrow);
    }
    let to_monday = (7 - now.weekday().num_days_from_monday()) % 7;
    let to_monday = if to_monday == 0 { 7 } else { to_monday };
    let monday = at(
        &(now.clone() + Duration::days(i64::from(to_monday))),
        MORNING_HOUR,
    );
    if let Some(monday) = monday.filter(|monday| Some(monday) != tomorrow.as_ref()) {
        push("overview-preset-next-week", weekday_time(&monday), monday);
    }
    presets
}

/// How long ago `since` was, in its largest unit: `now`, `4m`, `2h`, `3d`.
pub(crate) fn elapsed(since: SystemTime, now: SystemTime) -> String {
    let seconds = now
        .duration_since(since)
        .map_or(0, |elapsed| elapsed.as_secs());
    match seconds {
        0..60 => fl!("overview-elapsed-now"),
        60..3600 => fl!("overview-elapsed-minutes", minutes = seconds / 60),
        3600..86_400 => fl!("overview-elapsed-hours", hours = seconds / 3600),
        _ => fl!("overview-elapsed-days", days = seconds / 86_400),
    }
}

/// When a snooze ends, as its row says it: `18:00` today, else `Tue 09:00`.
pub(crate) fn wakes<Tz: TimeZone>(until: SystemTime, now: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let until = DateTime::<chrono::Utc>::from(until).with_timezone(&now.timezone());
    if until.date_naive() == now.date_naive() {
        until.format("%H:%M").to_string()
    } else {
        weekday_time(&until)
    }
}

/// `Mon 09:00`, with the weekday in the shown language.
fn weekday_time<Tz: TimeZone>(time: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let day = match time.weekday() {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    };
    fl!(
        "overview-weekday-time",
        day = day,
        time = time.format("%H:%M").to_string()
    )
}

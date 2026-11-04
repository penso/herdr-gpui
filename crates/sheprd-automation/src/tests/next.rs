use super::*;
use jiff::{Timestamp, tz::TimeZone};

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn daily(hour: u8, minute: u8, days: Days) -> Schedule {
    Schedule::At {
        time: ClockTime { hour, minute },
        days,
    }
}

fn paris() -> TimeZone {
    TimeZone::get("Europe/Paris").unwrap()
}

#[test]
fn a_time_of_day_fires_in_its_zone() {
    let next = daily(3, 0, Days::ALL).next_after(&paris(), at("2026-10-22T00:00:00Z"), None);
    // 03:00 in Paris is 01:00 UTC under summer time.
    assert_eq!(next, Some(at("2026-10-22T01:00:00Z")));
    let after_it = daily(3, 0, Days::ALL).next_after(&paris(), at("2026-10-22T01:00:00Z"), None);
    assert_eq!(after_it, Some(at("2026-10-23T01:00:00Z")));
}

#[test]
fn days_skip_to_the_next_allowed_one() {
    // Friday 2026-10-23 after 03:00 UTC: the next weekday is Monday.
    let next =
        daily(3, 0, Days::WEEKDAYS).next_after(&TimeZone::UTC, at("2026-10-23T04:00:00Z"), None);
    assert_eq!(next, Some(at("2026-10-26T03:00:00Z")));
}

#[test]
fn a_time_lost_to_daylight_saving_fires_after_the_gap() {
    // Paris skips 02:00-03:00 on 2026-03-29; 02:30 becomes 03:30 local.
    let next = daily(2, 30, Days::ALL).next_after(&paris(), at("2026-03-29T00:00:00Z"), None);
    assert_eq!(next, Some(at("2026-03-29T01:30:00Z")));
}

#[test]
fn an_interval_keeps_its_phase() {
    let every = Schedule::Every { seconds: 6 * 3_600 };
    let anchor = at("2026-10-22T00:00:00Z");
    let next = every.next_after(&TimeZone::UTC, at("2026-10-22T13:00:00Z"), Some(anchor));
    assert_eq!(next, Some(at("2026-10-22T18:00:00Z")));
    let fresh = every.next_after(&TimeZone::UTC, at("2026-10-22T13:00:00Z"), None);
    assert_eq!(fresh, Some(at("2026-10-22T19:00:00Z")));
}

#[test]
fn cron_jumps_over_days_and_hours() {
    let cron = Schedule::Cron {
        cron: Cron::parse("*/15 9-17 * * mon-fri").unwrap(),
    };
    let next = cron.next_after(&TimeZone::UTC, at("2026-10-23T17:50:00Z"), None);
    assert_eq!(next, Some(at("2026-10-26T09:00:00Z")));
    let soon = cron.next_after(&TimeZone::UTC, at("2026-10-22T09:01:30Z"), None);
    assert_eq!(soon, Some(at("2026-10-22T09:15:00Z")));
}

#[test]
fn an_impossible_cron_never_fires() {
    let never = Schedule::Cron {
        cron: Cron::parse("0 0 31 2 *").unwrap(),
    };
    assert_eq!(
        never.next_after(&TimeZone::UTC, at("2026-10-22T00:00:00Z"), None),
        None
    );
}

#[test]
fn manual_triggers_never_come_due() {
    let checked = at("2026-10-01T00:00:00Z");
    assert_eq!(
        Schedule::Manual.next_after(&TimeZone::UTC, checked, None),
        None
    );
    assert_eq!(
        due(
            &Schedule::Manual,
            &TimeZone::UTC,
            checked,
            None,
            at("2026-10-22T00:00:00Z")
        ),
        None
    );
}

#[test]
fn missed_firings_collapse_into_one_late_firing() {
    let schedule = daily(3, 0, Days::ALL);
    let found = due(
        &schedule,
        &TimeZone::UTC,
        at("2026-10-20T00:00:00Z"),
        None,
        at("2026-10-23T05:00:00Z"),
    );
    assert_eq!(
        found,
        Some(Due {
            scheduled: at("2026-10-23T03:00:00Z"),
            late: true,
        })
    );
}

#[test]
fn an_on_time_firing_is_not_late() {
    let found = due(
        &daily(3, 0, Days::ALL),
        &TimeZone::UTC,
        at("2026-10-23T02:59:00Z"),
        None,
        at("2026-10-23T03:00:30Z"),
    );
    assert_eq!(
        found,
        Some(Due {
            scheduled: at("2026-10-23T03:00:00Z"),
            late: false,
        })
    );
    let nothing = due(
        &daily(3, 0, Days::ALL),
        &TimeZone::UTC,
        at("2026-10-23T03:00:00Z"),
        None,
        at("2026-10-23T05:00:00Z"),
    );
    assert_eq!(nothing, None);
}

#[test]
fn a_long_outage_catches_up_with_the_latest_missed_firing() {
    let checked = at("2026-09-01T00:00:00Z");
    let now = at("2026-10-01T12:34:30Z");
    let every = Schedule::Every { seconds: 60 };
    let due = due(&every, &TimeZone::UTC, checked, Some(checked), now).unwrap();
    assert_eq!(due.scheduled, at("2026-10-01T12:34:00Z"));
    let cron = Schedule::Cron {
        cron: Cron::parse("* * * * *").unwrap(),
    };
    let due = due_cron(&cron, checked, now);
    assert_eq!(due, Some(at("2026-10-01T12:34:00Z")));
}

fn due_cron(cron: &Schedule, checked: Timestamp, now: Timestamp) -> Option<Timestamp> {
    due(cron, &TimeZone::UTC, checked, None, now).map(|due| due.scheduled)
}

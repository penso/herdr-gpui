use super::*;
use crate::overview::presets::{elapsed, presets, wakes};
use chrono::{FixedOffset, TimeZone};

/// Drops Fluent's isolation marks around placeables.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn local(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::DateTime<FixedOffset> {
    FixedOffset::east_opt(2 * 3600)
        .unwrap()
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn labels(now: &chrono::DateTime<FixedOffset>) -> Vec<(&'static str, String)> {
    presets(now)
        .into_iter()
        .map(|preset| (preset.label, plain(preset.when)))
        .collect()
}

#[test]
fn a_weekday_afternoon_offers_every_preset() {
    // Wednesday 14 October 2026, 14:00.
    let now = local(2026, 10, 14, 14, 0);
    assert_eq!(
        labels(&now),
        [
            ("overview-preset-hour", "15:00".to_owned()),
            ("overview-preset-hours", "17:00".to_owned()),
            ("overview-preset-evening", "18:00".to_owned()),
            ("overview-preset-tomorrow", "09:00".to_owned()),
            ("overview-preset-next-week", "Mon 09:00".to_owned()),
        ]
    );
    let tomorrow = presets(&now)
        .into_iter()
        .find(|preset| preset.label == "overview-preset-tomorrow")
        .unwrap();
    assert_eq!(tomorrow.until, SystemTime::from(local(2026, 10, 15, 9, 0)));
}

#[test]
fn this_evening_needs_an_hour_and_next_week_must_not_be_tomorrow() {
    // Sunday 18 October 2026, 17:30: the evening is half an hour away, and
    // Monday morning is tomorrow.
    let now = local(2026, 10, 18, 17, 30);
    let offered: Vec<_> = labels(&now).into_iter().map(|(label, _)| label).collect();
    assert_eq!(
        offered,
        [
            "overview-preset-hour",
            "overview-preset-hours",
            "overview-preset-tomorrow"
        ]
    );
}

#[test]
fn times_read_in_their_largest_unit() {
    let elapsed = |since, now| plain(elapsed(since, now));
    assert_eq!(elapsed(at(100), at(130)), "now");
    assert_eq!(elapsed(at(0), at(4 * 60 + 59)), "4m");
    assert_eq!(elapsed(at(0), at(2 * 3600)), "2h");
    assert_eq!(elapsed(at(0), at(3 * 86_400)), "3d");
    // A clock that went back reads as now.
    assert_eq!(elapsed(at(100), at(50)), "now");
}

#[test]
fn a_wake_time_names_its_day_unless_it_is_today() {
    let now = local(2026, 10, 14, 14, 0);
    assert_eq!(
        wakes(SystemTime::from(local(2026, 10, 14, 18, 0)), &now),
        "18:00"
    );
    assert_eq!(
        plain(wakes(SystemTime::from(local(2026, 10, 19, 9, 0)), &now)),
        "Mon 09:00"
    );
}

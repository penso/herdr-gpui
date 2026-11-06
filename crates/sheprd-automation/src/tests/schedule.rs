use super::*;
use std::time::Duration;

#[test]
fn durations_read_and_print() {
    assert_eq!(parse_duration("90s"), Some(Duration::from_secs(90)));
    assert_eq!(parse_duration("1h30m"), Some(Duration::from_secs(5_400)));
    assert_eq!(parse_duration("2d"), Some(Duration::from_secs(172_800)));
    for bad in ["", "5", "m", "1x", "0m", "1h 30m", "-5m"] {
        assert_eq!(parse_duration(bad), None, "{bad}");
    }
    assert_eq!(format_duration(Duration::from_secs(5_400)), "1h30m");
    assert_eq!(format_duration(Duration::from_secs(90_061)), "1d1h1m1s");
}

#[test]
fn clock_times_are_strict() {
    assert_eq!(
        ClockTime::parse("00:00"),
        Some(ClockTime { hour: 0, minute: 0 })
    );
    assert_eq!(
        ClockTime::parse("23:59"),
        Some(ClockTime {
            hour: 23,
            minute: 59
        })
    );
    for bad in ["24:00", "12:60", "3:00", "03:0", "0300", "ab:cd"] {
        assert_eq!(ClockTime::parse(bad), None, "{bad}");
    }
}

#[test]
fn days_read_and_print() {
    assert_eq!(Days::parse("daily"), Some(Days::ALL));
    assert_eq!(Days::parse("mon-fri"), Some(Days::WEEKDAYS));
    assert_eq!(Days::parse("sat, sun"), Some(Days::WEEKENDS));
    let some = Days::parse("mon,wed-thu").unwrap();
    assert_eq!(some.to_string(), "mon,wed,thu");
    assert!(some.contains(0) && some.contains(2) && !some.contains(1));
    for bad in ["", "fri-mon", "monday", "mon,,tue"] {
        assert_eq!(Days::parse(bad), None, "{bad}");
    }
}

#[test]
fn cron_fields_match() {
    let quarter = Cron::parse("*/15 9-17 * * mon-fri").unwrap();
    // 2026-10-22 is a Thursday (weekday 4 from Sunday).
    assert!(quarter.matches(30, 9, 22, 10, 4));
    assert!(!quarter.matches(31, 9, 22, 10, 4));
    assert!(!quarter.matches(30, 18, 22, 10, 4));
    assert!(!quarter.matches(30, 9, 25, 10, 0));

    let sunday = Cron::parse("0 3 * * 7").unwrap();
    assert!(sunday.matches(0, 3, 25, 10, 0));

    // Both day fields restricted: either one matching is enough, as in cron.
    let either = Cron::parse("0 0 1 * mon").unwrap();
    assert!(either.matches(0, 0, 1, 10, 4));
    assert!(either.matches(0, 0, 5, 10, 1));
    assert!(!either.matches(0, 0, 5, 10, 2));

    let months = Cron::parse("0 0 1 jan,jul *").unwrap();
    assert!(months.matches(0, 0, 1, 7, 3));
    assert!(!months.matches(0, 0, 1, 8, 3));
}

#[test]
fn cron_refuses_bad_fields() {
    for bad in [
        "* * * *",
        "60 * * * *",
        "* 24 * * *",
        "* * 0 * *",
        "*/0 * * * *",
        "5-1 * * * *",
        "* * * smarch *",
    ] {
        assert!(Cron::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn zones_come_from_the_database() {
    assert_eq!(
        Zone::named("Europe/Paris"),
        Some(Zone::Named("Europe/Paris".to_owned()))
    );
    assert_eq!(Zone::named("Mars/Olympus"), None);
}

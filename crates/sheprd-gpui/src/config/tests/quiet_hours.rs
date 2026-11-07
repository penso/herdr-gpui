#![allow(clippy::unwrap_used)]

use crate::config::QuietHours;

fn at(hour: u16, minute: u16) -> u16 {
    hour * 60 + minute
}

#[test]
fn a_window_across_midnight_holds_both_ends_of_the_day() {
    let hours = QuietHours::parse("22:00-08:00").unwrap();
    assert!(hours.holds(at(22, 0)));
    assert!(hours.holds(at(23, 59)));
    assert!(hours.holds(at(0, 0)));
    assert!(hours.holds(at(7, 59)));
    assert!(!hours.holds(at(8, 0)));
    assert!(!hours.holds(at(21, 59)));
}

#[test]
fn a_window_within_the_day_ends_where_it_says() {
    let hours = QuietHours::parse("12:30-13:15").unwrap();
    assert!(hours.holds(at(12, 30)));
    assert!(hours.holds(at(13, 14)));
    assert!(!hours.holds(at(13, 15)));
    assert!(!hours.holds(at(12, 29)));
}

#[test]
fn a_window_that_ends_where_it_starts_is_empty() {
    let hours = QuietHours::parse("09:00-09:00").unwrap();
    assert!((0..24 * 60).all(|minute| !hours.holds(minute)));
}

#[test]
fn it_reads_back_what_it_writes() {
    for text in ["22:00-08:00", "00:05-23:59"] {
        assert_eq!(QuietHours::parse(text).unwrap().to_string(), text);
    }
    assert_eq!(QuietHours::at_hours(22, 7).to_string(), "22:00-07:00");
}

#[test]
fn it_refuses_what_is_not_a_window() {
    for text in [
        "",
        "22:00",
        "22-08",
        "24:00-08:00",
        "22:60-08:00",
        "aa:bb-cc:dd",
        "-1:00-08:00",
    ] {
        assert_eq!(QuietHours::parse(text), None, "{text}");
    }
}

#[derive(serde::Deserialize)]
struct Table {
    quiet_hours: QuietHours,
}

#[test]
fn a_bad_value_in_the_file_says_what_is_expected() {
    let error = toml::from_str::<Table>("quiet_hours = \"late\"")
        .err()
        .map(|error| error.to_string())
        .unwrap();
    assert!(error.contains("22:00-08:00"), "{error}");
    let table: Table = toml::from_str("quiet_hours = \"23:00-07:30\"").unwrap();
    assert_eq!(table.quiet_hours.to_string(), "23:00-07:30");
}

#[path = "../release_version.rs"]
mod release_version;

use release_version::numeric;

fn packed(year: u64, month_day: u64, counter: u64) -> u64 {
    (year << 48) | (month_day << 32) | (counter << 16)
}

#[test]
fn calendar_release_packs_as_year_month_day_counter_zero() {
    assert_eq!(numeric("20261016.3"), Some(packed(2026, 1016, 3)));
    assert_eq!(numeric("20261231.65535"), Some(packed(2026, 1231, 65535)));
}

#[test]
fn counter_over_sixteen_bits_is_rejected() {
    assert_eq!(numeric("20261016.65536"), None);
}

#[test]
fn malformed_versions_are_rejected() {
    for version in [
        "",
        "2026101.3",
        "202610160.3",
        "20261016",
        "20261016.",
        "v20261016.3",
        "20261016.+3",
        "20261016.3.1",
        "2026-10-16.3",
    ] {
        assert_eq!(numeric(version), None, "{version:?}");
    }
}

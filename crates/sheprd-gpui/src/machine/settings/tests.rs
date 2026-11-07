#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_display_reads_back_as_it_was_written() {
    for display in [Display::Fit, Display::PRESETS[0], Display::PRESETS[2]] {
        assert_eq!(display.to_string().parse::<Display>().unwrap(), display);
    }
}

#[test]
fn a_display_size_must_be_two_sensible_numbers() {
    for text in [
        "",
        "big",
        "1920",
        "1920x",
        "x1200",
        "0x0",
        "99999x1200",
        "1x1",
    ] {
        assert!(
            matches!(text.parse::<Display>(), Err(Error::InvalidDisplay)),
            "{text:?}"
        );
    }
}

#[test]
fn limits_follow_the_host_and_leave_it_memory() {
    let limits = Limits::of(16, 64 << 30);
    assert_eq!(limits.cpus, 2..=16);
    assert_eq!(limits.memory_gib, 4..=60);
}

#[test]
fn limits_stay_a_valid_range_on_a_small_host() {
    let limits = Limits::of(1, 4 << 30);
    assert_eq!(limits.cpus, 2..=2);
    assert_eq!(limits.memory_gib, 4..=4);
}

#[test]
fn settings_beyond_the_limits_are_refused_by_what_is_wrong() {
    let limits = Limits::of(8, 32 << 30);
    let fine = Settings::default();
    assert!(limits.check(&fine).is_ok());
    let cpus = Settings { cpus: 9, ..fine };
    assert!(matches!(
        limits.check(&cpus),
        Err(Error::Cpus { min: 2, max: 8 })
    ));
    let memory = Settings {
        memory_gib: 3,
        ..fine
    };
    assert!(matches!(
        limits.check(&memory),
        Err(Error::Memory { min: 4, max: 28 })
    ));
}

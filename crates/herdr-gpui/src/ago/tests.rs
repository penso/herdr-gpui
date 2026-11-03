use super::*;
use herdr_i18n::{Locale, Pseudo};

#[test]
fn reads_in_the_largest_whole_unit() {
    for (seconds, text) in [
        (0, "just now"),
        (59, "just now"),
        (60, "1 min ago"),
        (59 * 60, "59 min ago"),
        (3600, "1 h ago"),
        (23 * 3600, "23 h ago"),
        (86_400, "yesterday"),
        (2 * 86_400 - 1, "yesterday"),
        (3 * 86_400, "3 days ago"),
        (21 * 86_400, "21 days ago"),
    ] {
        assert_eq!(english(seconds), text, "{seconds}");
    }
}

#[test]
fn a_clock_ahead_reads_as_now() {
    assert_eq!(since(130, 100), 0);
    assert_eq!(since(0, 125), 125);
    assert_eq!(english(since(100, 0)), "just now");
}

#[test]
fn the_window_isolates_the_count_and_translates_every_unit() {
    i18n::show(&Locale::FALLBACK);
    assert_eq!(text(120), "\u{2068}2\u{2069} min ago");
    i18n::show(&Pseudo::Accented.locale());
    for seconds in [0, 120, 7200, 86_400, 3 * 86_400] {
        let shown = text(seconds);
        assert!(herdr_i18n::pseudo::is_marked(&shown), "{shown}");
    }
    i18n::show(&Locale::FALLBACK);
}

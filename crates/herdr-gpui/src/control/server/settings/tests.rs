use super::{
    caffeine::{Timer, remaining, set_timed, supersede, timed},
    saves::font_names,
};
use crate::caffeine::{Cup, Mode};
use std::time::{Duration, Instant};

#[test]
fn a_timed_hold_ends_only_the_hold_it_was_set_on() {
    let now = Instant::now();
    let timer = Timer {
        changes: 3,
        until: now + Duration::from_secs(60),
    };
    assert_eq!(
        remaining(timer, 3, Cup::On(Mode::Indefinite), now),
        Some(Duration::from_secs(60))
    );
    // Due: the hold ends.
    assert_eq!(
        remaining(
            timer,
            3,
            Cup::On(Mode::Indefinite),
            now + Duration::from_secs(61)
        ),
        Some(Duration::ZERO)
    );
    // Anyone changing caffeine since, even back on, cancels the timer.
    assert_eq!(
        remaining(
            timer,
            4,
            Cup::On(Mode::Indefinite),
            now + Duration::from_secs(61)
        ),
        None
    );
    assert_eq!(remaining(timer, 5, Cup::On(Mode::Indefinite), now), None);
    // A hold already released, or changing, is left alone.
    assert_eq!(
        remaining(timer, 3, Cup::Off, now + Duration::from_secs(61)),
        None
    );
    assert_eq!(remaining(timer, 3, Cup::Pending, now), None);
    // A hold until agents finish is the window's, never a timer's to end.
    assert_eq!(
        remaining(timer, 3, Cup::On(Mode::UntilAgentsFinish), now),
        None
    );
}

#[gpui::test]
fn an_untimed_request_cancels_an_earlier_timer(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let timer = Timer {
            changes: 0,
            until: Instant::now() + Duration::from_secs(60),
        };
        set_timed(timer, cx);
        // Another timed request replaces the timer itself, later.
        supersede(Some(30), cx);
        assert!(timed(cx));
        supersede(None, cx);
        assert!(!timed(cx));
    });
}

#[test]
fn installed_fonts_are_sorted_deduplicated_and_not_filtered_by_face() {
    assert_eq!(
        font_names(["Zed", "Alpha", "Zed", "Mono", " "].map(str::to_owned)),
        vec!["Alpha", "Mono", "Zed"]
    );
}

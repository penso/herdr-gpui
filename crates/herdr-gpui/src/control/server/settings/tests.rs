use super::{
    caffeine::{Timer, remaining, set_timed, state_name, supersede, timed},
    keys::reach_name,
    saves::font_names,
    services::describe,
};
use crate::{
    caffeine::{Cup, Mode},
    keymap::Reach,
    updater::State,
};
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

#[test]
fn caffeine_states_have_stable_names() {
    assert_eq!(state_name(Cup::Off), "off");
    assert_eq!(state_name(Cup::Pending), "pending");
    assert_eq!(state_name(Cup::On(Mode::Indefinite)), "on");
    assert_eq!(
        state_name(Cup::On(Mode::UntilAgentsFinish)),
        "until_agents_finish"
    );
}

#[test]
fn every_reach_has_a_stable_name() {
    let reaches = [
        (Reach::Runs, "runs"),
        (Reach::Shadowed, "shadowed"),
        (Reach::Taken, "taken"),
        (Reach::NeedsModifier, "needs_modifier"),
        (Reach::NoPrefix, "no_prefix"),
        (Reach::OverLimit, "over_limit"),
        (Reach::Unsupported, "unsupported"),
    ];
    for (reach, name) in reaches {
        assert_eq!(reach_name(reach), name);
    }
}

#[test]
fn update_states_are_named_with_their_version_or_reason() {
    assert_eq!(
        describe(&State::Disabled("Local build".into())),
        ("disabled", Some("Local build".to_owned()), None)
    );
    assert_eq!(describe(&State::Checking), ("checking", None, None));
    assert_eq!(
        describe(&State::Available {
            version: "1.2.0".into(),
            notes: "notes".into()
        }),
        ("available", None, Some("1.2.0"))
    );
    assert_eq!(
        describe(&State::Error("offline".into())),
        ("error", Some("offline".to_owned()), None)
    );
    assert_eq!(
        describe(&State::Upgrading {
            detail: crate::updater::Progress::Starting
        }),
        ("upgrading", Some("Starting Homebrew...".to_owned()), None)
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

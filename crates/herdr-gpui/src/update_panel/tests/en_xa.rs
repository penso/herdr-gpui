//! The `en-XA` leak test (`crate::i18n::leak_tests`) for App Updates in
//! each state: versions, the release notes, which come from GitHub, and the
//! lines a package manager prints are data.
#![allow(clippy::unwrap_used)]

use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use crate::updater::{PREVIEW_NOTES, Progress, State};
use crate::{APP_VERSION, release_notes};
use gpui::{px, size};

/// A line Homebrew or a system package printed.
const OUTPUT: &str = "==> Downloading herdr-gpui";

fn states() -> Vec<State> {
    let version = || "9999.0.0".to_owned();
    let upgrading = [
        Progress::Approve,
        Progress::Asking,
        Progress::Checking,
        Progress::Homebrew(OUTPUT.into()),
        Progress::Package(OUTPUT.into()),
        Progress::Preparing,
        Progress::Refreshing,
        Progress::Retrying,
        Progress::Starting,
    ]
    .into_iter()
    .map(|detail| State::Upgrading { detail });
    let mut states = vec![
        State::Disabled("Local builds never check.".into()),
        State::Idle,
        State::Checking,
        State::Current,
        State::Available {
            version: version(),
            notes: PREVIEW_NOTES.into(),
        },
        State::Downloading {
            received: 25_000_000,
            total: 100_000_000,
        },
        State::Downloading {
            received: 25_000_000,
            total: 0,
        },
        State::Ready { version: version() },
        State::Installing,
        State::Homebrew {
            version: version(),
            notes: String::new(),
        },
        State::Restart { version: version() },
        State::Restarting,
        State::Cancelling,
    ];
    states.extend(upgrading);
    states
}

#[test]
fn every_update_state_shows_only_translated_text_under_en_xa() {
    under_en_xa("App Updates en-XA leak", |cx, recorder| {
        let mut data: Vec<String> = release_notes::parse(PREVIEW_NOTES)
            .into_iter()
            .map(|line| line.text)
            .collect();
        data.extend([
            APP_VERSION.to_owned(),
            "9999.0.0".to_owned(),
            OUTPUT.to_owned(),
        ]);
        for state in states() {
            let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
            cx.simulate_resize(size(px(1200.), px(1600.)));
            draw(cx);
            recorder.take();
            let name = format!("App Updates, {state:?}");
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.open_app_update(false, window, cx);
                    view.update_preview = Some(state);
                    cx.notify();
                })
            });
            draw(cx);
            assert_translated(&name, &recorder.take(), &data);
        }
    });
}

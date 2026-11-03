//! When each activity bar section was last open is remembered with the rest
//! of the chrome, so a count stays muted across restarts until it changes.
use super::*;
use crate::sidebar::ActivityPanel;
use chrono::DateTime;
// Over the glob's GPUI `test`, which `#[gpui::test]` would expand into.
use core::prelude::v1::test;

#[test]
fn last_opened_times_round_trip_and_damaged_ones_are_forgotten() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    let at = |text: &str| DateTime::parse_from_rfc3339(text).unwrap().to_utc();
    let chrome = Chrome {
        activity_last_opened: [
            (ActivityPanel::PullRequests, at("2026-10-19T08:30:00Z")),
            (ActivityPanel::Security, at("2026-10-18T22:00:00.250Z")),
        ]
        .into(),
        ..Chrome::default()
    };
    write_chrome(&path, chrome.clone()).unwrap();
    assert_eq!(read_chrome(&path).unwrap(), chrome);

    // Earlier builds wrote none; an unknown section or unreadable time is
    // dropped alone, keeping the rest of the file.
    fs::write(
        &path,
        r#"{"sidebar_width_px": 250, "activity_last_opened_at": {
            "issues": "2026-10-19T10:00:00+02:00",
            "dashboard": "2026-10-19T10:00:00Z",
            "overview": "yesterday",
            "security": 3
        }}"#,
    )
    .unwrap();
    let read = read_chrome(&path).unwrap();
    assert_eq!(
        read.activity_last_opened,
        [(ActivityPanel::Issues, at("2026-10-19T08:00:00Z"))].into()
    );
    assert_eq!(read.sidebar_width, Some(250.));
    for stored in ["", r#", "activity_last_opened_at": []"#] {
        fs::write(&path, format!(r#"{{"sidebar_width_px": 250{stored}}}"#)).unwrap();
        assert!(read_chrome(&path).unwrap().activity_last_opened.is_empty());
    }
}

/// A section picked before the file loaded queues a save holding only this
/// run's visit, which overwrites the stored ones; once they load and merge,
/// the window saves the merge, so neither is lost.
#[gpui::test]
fn visits_stored_before_a_save_that_raced_the_load_are_saved_again(cx: &mut TestAppContext) {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    let stored = DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
        .unwrap()
        .to_utc();
    write_chrome(
        &path,
        Chrome {
            activity_last_opened: [(ActivityPanel::Security, stored)].into(),
            sidebar_width: Some(250.),
            ..Chrome::default()
        },
    )
    .unwrap();
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let chrome = view.update(cx, |view, _| {
        let mut preferences = Preferences::start(Ok(path.clone()));
        let chrome = await_loaded(&mut preferences);
        view.sidebar_preferences = Some(preferences);
        // The pick's save reaches the worker after its load.
        view.activity.pick(ActivityPanel::Issues);
        view.save_chrome();
        chrome
    });
    view.update(cx, |view, _| view.apply_stored_chrome(chrome));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let read = read_chrome(&path).ok();
        let last_opened = read.as_ref().map(|chrome| &chrome.activity_last_opened);
        if last_opened.is_some_and(|last_opened| {
            last_opened.get(&ActivityPanel::Security) == Some(&stored)
                && last_opened.contains_key(&ActivityPanel::Issues)
        }) {
            assert_eq!(read.unwrap().sidebar_width, Some(250.));
            break;
        }
        assert!(Instant::now() < deadline, "merged visits were not saved");
        thread::sleep(Duration::from_millis(5));
    }
    // A load with nothing new to merge asks for no save.
    view.update(cx, |view, _| {
        let last_opened = view.activity.last_opened().clone();
        assert!(!view.activity.restore(None, &last_opened));
    });
}

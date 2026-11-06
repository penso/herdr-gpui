use super::*;

fn available() -> State {
    State::Available {
        version: "20261106.1".into(),
        notes: String::new(),
    }
}

#[test]
fn an_update_asks_for_the_latest_release_first() {
    for state in [State::Idle, State::Current] {
        assert_eq!(flow(&state, false), Flow::Check);
        assert_eq!(flow(&state, true), Flow::Stop);
    }
}

#[test]
fn an_update_walks_every_step_to_the_restart() {
    let version = || "20261106.1".to_owned();
    assert_eq!(flow(&available(), true), Flow::Download);
    assert_eq!(
        flow(&State::Ready { version: version() }, true),
        Flow::Install
    );
    assert_eq!(
        flow(
            &State::Homebrew {
                version: version(),
                notes: String::new()
            },
            true
        ),
        Flow::Upgrade
    );
    assert_eq!(
        flow(&State::Restart { version: version() }, true),
        Flow::Restart
    );
    assert_eq!(flow(&State::Restarting, true), Flow::Stop);
}

#[test]
fn an_update_waits_while_the_updater_works() {
    assert_eq!(flow(&State::Checking, true), Flow::Wait);
    assert_eq!(flow(&State::Installing, true), Flow::Wait);
    assert_eq!(
        flow(
            &State::Downloading {
                received: 1,
                total: 2
            },
            true
        ),
        Flow::Wait
    );
}

#[test]
fn a_build_that_cannot_update_stops_at_once() {
    assert_eq!(
        flow(&State::Disabled(Disabled::LocalBuild), false),
        Flow::Stop
    );
}

#[test]
fn an_updater_that_cannot_update_is_told_as_such() {
    let update = report(&Updater::secondary(), false);
    assert_eq!(update.stage, Stage::Disabled);
    assert!(update.detail.is_some());
}

#[test]
fn a_report_goes_stale_once_its_window_stops_renewing_it() {
    let update = AppUpdate {
        stage: Stage::Idle,
        latest: None,
        detail: None,
    };
    let mut updates = Updates {
        reported: Some((update.clone(), Instant::now())),
        running: None,
    };
    assert_eq!(updates.outgoing(), Some(update.clone()));
    let Some(old) = Instant::now().checked_sub(REPORT_LIFE) else {
        return;
    };
    updates.reported = Some((update, old));
    assert_eq!(updates.outgoing(), None);
}

#[test]
fn a_second_request_does_not_restart_a_running_update() {
    let mut updates = Updates {
        reported: None,
        running: Some(true),
    };
    updates.requested();
    assert_eq!(updates.running, Some(true));
}

#[test]
fn text_is_cut_on_a_character_boundary() {
    let cut = bounded("é".repeat(MAX_UPDATE_TEXT));
    assert!(cut.len() <= MAX_UPDATE_TEXT);
    assert!(cut.chars().all(|character| character == 'é'));
}

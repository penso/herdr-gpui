use super::*;

#[test]
fn a_new_failure_with_the_same_words_is_a_change() {
    let mut updater = Updater::default();
    let failure = Failure::from(Error::Cancelled);
    assert_eq!(failure, failure.clone(), "a clone is the same failure");
    assert_ne!(failure, Failure::from(Error::Cancelled));
    for _ in 0..2 {
        publish(
            &updater.mailbox,
            updater.generation,
            State::Error(Error::Cancelled.into()),
            None,
        );
        assert!(updater.poll(), "each failure redraws App Updates");
    }
    assert!(
        matches!(&updater.state, State::Error(failure) if matches!(**failure, Error::Cancelled))
    );
}

#[test]
fn reasons_stay_typed_and_keep_their_english() {
    assert_eq!(
        Updater::default().state,
        State::Disabled(Disabled::LocalBuild)
    );
    assert_eq!(
        Updater::secondary().state,
        State::Disabled(Disabled::Secondary)
    );
    assert_eq!(
        Disabled::Secondary.to_string(),
        "another window owns app updates in this process. Use the window that was open first."
    );
    let start = Disabled::Unsupported(Error::WorkerStopped.into());
    assert_eq!(start.to_string(), "Update worker stopped");
}

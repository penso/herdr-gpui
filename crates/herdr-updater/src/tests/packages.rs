use super::*;

#[test]
fn known_release_survives_progress_and_errors() {
    let mut updater = Updater::default();
    for state in [
        State::Available {
            version: "20261010.2".into(),
            notes: String::new(),
        },
        State::Downloading {
            received: 10,
            total: 100,
        },
        State::Error(Error::PackageAuthorization.into()),
    ] {
        publish(&updater.mailbox, updater.generation, state, None);
        updater.poll();
        assert_eq!(updater.latest_version(), Some("20261010.2"));
    }
}

#[test]
fn an_installed_package_keeps_its_restart_offer_and_install_is_not_interruptible()
-> anyhow::Result<()> {
    let (sender, receiver) = mpsc::sync_channel(2);
    let mut updater = Updater::default();
    updater.commands = Some(sender);
    updater.state = State::Ready {
        version: "20261010.2".into(),
    };
    updater.install();
    assert!(matches!(receiver.try_recv()?.operation, Operation::Install));
    assert!(!updater.interruptible());
    updater.cancel();
    assert!(!updater.cancelled.load(Ordering::Acquire));
    updater.state = State::Restart {
        version: "20261010.2".into(),
    };
    updater.set_channel(UpdateChannel::Beta);
    updater.check();
    updater.poll();
    assert!(receiver.try_recv().is_err());
    assert!(matches!(updater.state, State::Restart { .. }));
    Ok(())
}

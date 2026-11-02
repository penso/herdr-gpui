use super::*;

#[cfg(unix)]
#[test]
fn a_running_server_is_saved_with_a_warning_after_the_cli_refuses_a_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let binary = root.path().join("herdr");
    crate::test_executable::write(
        &binary,
        "#!/bin/sh\nprintf '%s\\n' 'remote herdr server is still running v0.9.4; it will use 0.9.3 after it restarts.' \
         'error: remote server is not ready for saved machines; machine was not saved' >&2\nexit 1\n",
        0o700,
    )?;
    let request = Request::new("version-difference.test", "Remote", "work")?;
    let claim = Claim::acquire(destination("version-difference.test"), "work")?;
    let result = save_with(&binary, &request, &claim, || Ok(Vec::new()), |_| None);
    let stored = std::cell::Cell::new(false);
    let outcome = finish_save(
        result,
        || Ok(HostProbe::Running),
        || {
            stored.set(true);
            Ok(())
        },
    )?;
    assert!(stored.get());
    assert_eq!(outcome, SaveOutcome::ExistingServer);
    assert!(
        outcome
            .warning()
            .is_some_and(|id| crate::i18n::message(id, &[]).starts_with("Device added"))
    );
    Ok(())
}

#[test]
fn successful_cli_save_needs_no_fallback_or_warning() -> Result<()> {
    let outcome = finish_save(
        Ok(()),
        || panic!("unexpected probe"),
        || panic!("unexpected save"),
    )?;
    assert_eq!(outcome, SaveOutcome::Ready);
    assert_eq!(outcome.warning(), None);
    Ok(())
}

#[test]
fn restart_refusal_only_falls_back_for_a_still_running_host() {
    for probe in [
        HostProbe::Missing,
        HostProbe::Outdated,
        HostProbe::Stopped,
        HostProbe::SshFailed(herdr_client::SshFailure::Auth),
    ] {
        let result = finish_save(
            Err(Error::ServerRestart.into()),
            || Ok(probe),
            || panic!("must not save"),
        );
        assert!(matches!(
            result,
            Err(crate::Error::Devices(Error::ServerRestart))
        ));
    }
    let result = finish_save(
        Err(Error::ServerRestart.into()),
        || Err(Error::SetupTimeout.into()),
        || panic!("must not save"),
    );
    assert!(matches!(
        result,
        Err(crate::Error::Devices(Error::SetupTimeout))
    ));
}

#[test]
fn unrelated_failures_and_failed_catalog_writes_stay_errors() {
    let result = finish_save(
        Err(Error::SetupTimeout.into()),
        || panic!("unexpected probe"),
        || panic!("must not save"),
    );
    assert!(matches!(
        result,
        Err(crate::Error::Devices(Error::SetupTimeout))
    ));
    let result = finish_save(
        Err(Error::ServerRestart.into()),
        || Ok(HostProbe::Running),
        || Err(Error::Exists("Already saved".into()).into()),
    );
    assert!(matches!(
        result,
        Err(crate::Error::Devices(Error::Exists(_)))
    ));
    let result = finish_save(
        Err(Error::ServerRestart.into()),
        || Ok(HostProbe::Running),
        || Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),
    );
    assert!(result.is_err());
}

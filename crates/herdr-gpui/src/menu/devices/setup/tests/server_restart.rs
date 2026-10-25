use super::*;

/// framework.local: a server Herdr wants restarted before saving makes
/// `machine add` refuse without a terminal, which the dialog explains.
#[test]
fn a_server_that_needs_a_restart_is_named_as_such() -> Result<()> {
    let root = tempfile::tempdir()?;
    let binary = root.path().join("herdr");
    crate::test_executable::write(
        &binary,
        "#!/bin/sh\nprintf '%s\\n' 'remote herdr server is currently running' \
         'error: remote server is not ready for saved machines; machine was not saved' >&2\nexit 1\n",
        0o700,
    )?;
    let request = Request::new("framework.local", "", "")?;
    let claim = Claim::acquire(destination("restart.test"), "default")?;
    let error = save_with(&binary, &request, &claim, || Ok(Vec::new()), |_| None).err();
    assert!(
        matches!(error, Some(crate::Error::Devices(Error::ServerRestart))),
        "{error:?}"
    );
    Ok(())
}

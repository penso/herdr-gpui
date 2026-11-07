use super::*;

#[test]
fn future_artifact_receipt_keeps_legacy_cask_ownership_and_relaunch_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let prefix = root.path().join("prefix");
    let bundle = root.path().join("Herdr.app");
    fs::create_dir_all(bundle.join("Contents/MacOS"))?;
    let executable = bundle.join("Contents/MacOS/sheprd");
    fs::write(&executable, b"future executable")?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    let link = caskroom(&prefix, "20260920.2", &bundle)?;
    fs::rename(&link, link.with_file_name("Sheprd.app"))?;
    let found = locate(
        &bundle,
        fs::metadata(root.path())?.uid(),
        root.path().into(),
        [prefix],
    )
    .ok_or(Error::NotHerdrBundle)?;
    assert_eq!(found.bundle, bundle);
    runnable(&found.bundle)?;
    assert!(
        command(&found)
            .get_program()
            .to_string_lossy()
            .ends_with("/bin/brew")
    );
    fs::remove_file(&executable)?;
    assert!(matches!(runnable(&bundle), Err(Error::RelaunchMissing(_))));
    Ok(())
}

use super::*;

#[test]
fn detection_names_the_cask_that_owns_the_bundle() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path().canonicalize()?;
    let prefix = root.join("prefix");
    let bundle = root.join("Applications/Sheprd.app");
    fs::create_dir_all(&bundle)?;
    let uid = fs::metadata(&root)?.uid();
    // Start from a legacy Caskroom that owns another bundle.
    caskroom(&prefix, "20260921.1", &root.join("Applications/Herdr.app"))?;
    let found = || locate(&bundle, uid, root.clone().into(), [prefix.clone()]);
    assert!(found().is_none(), "only the legacy cask is installed");

    let versioned = prefix.join("Caskroom/sheprd/20261110.1");
    fs::create_dir_all(&versioned)?;
    symlink(&bundle, versioned.join("Sheprd.app"))?;
    assert_eq!(found().map(|cask| cask.token), Some("sheprd"));
    Ok(())
}

#[test]
fn homebrew_is_asked_about_the_detected_cask() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let script = "#!/bin/sh\n\
        [ \"$*\" = 'list --cask --versions sheprd' ] || exit 1\n\
        echo 'herdr-gpui 20260921.1'\n\
        echo 'sheprd 20261110.1'\n";
    let mut sheprd = cask(script, root.path())?;
    sheprd.token = "sheprd";
    assert_eq!(installed(&sheprd, QUERY)?, "20261110.1");
    Ok(())
}

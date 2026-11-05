//! Following a cask that moved the running `Herdr.app` to `Sheprd.app`.
use super::*;

struct Moved {
    root: tempfile::TempDir,
    prefix: PathBuf,
    legacy: PathBuf,
    renamed: PathBuf,
}

/// A cask whose receipt already names `Sheprd.app`; the old bundle is gone.
fn moved() -> anyhow::Result<Moved> {
    let root = tempfile::tempdir()?;
    let prefix = root.path().join("prefix");
    let legacy = root.path().join("Applications/Herdr.app");
    let renamed = root.path().join("Applications/Sheprd.app");
    fs::create_dir_all(renamed.join("Contents/MacOS"))?;
    let link = caskroom(&prefix, "20261110.1", &renamed)?;
    fs::rename(&link, link.with_file_name("Sheprd.app"))?;
    Ok(Moved {
        root,
        prefix,
        legacy,
        renamed,
    })
}

impl Moved {
    fn locate(&self, bundle: &Path) -> anyhow::Result<Option<Cask>> {
        Ok(locate(
            bundle,
            fs::metadata(self.root.path())?.uid(),
            self.root.path().into(),
            [self.prefix.clone()],
        ))
    }
}

#[test]
fn cask_that_moved_the_bundle_is_followed_to_the_new_path() -> anyhow::Result<()> {
    let moved = moved()?;
    let cask = moved.locate(&moved.legacy)?.ok_or(Error::MissingCask)?;
    assert_eq!(cask.bundle, moved.renamed);
    assert_eq!(cask.legacy.as_ref(), Some(&moved.legacy));
    Ok(())
}

#[test]
fn followed_cask_leaves_the_old_path_resolving() -> anyhow::Result<()> {
    let moved = moved()?;
    let cask = moved.locate(&moved.legacy)?.ok_or(Error::MissingCask)?;
    link_legacy(&cask);
    assert_eq!(fs::read_link(&moved.legacy)?, Path::new("Sheprd.app"));
    assert!(moved.legacy.join("Contents/MacOS").is_dir());
    Ok(())
}

#[test]
fn old_path_that_is_already_linked_is_followed_and_left_alone() -> anyhow::Result<()> {
    let moved = moved()?;
    symlink("Sheprd.app", &moved.legacy)?;
    let cask = moved.locate(&moved.legacy)?.ok_or(Error::MissingCask)?;
    assert_eq!(cask.bundle, moved.renamed);
    assert_eq!(cask.legacy, None);
    Ok(())
}

#[test]
fn real_bundle_at_the_old_path_is_not_the_casks() -> anyhow::Result<()> {
    let moved = moved()?;
    fs::create_dir_all(&moved.legacy)?;
    assert!(moved.locate(&moved.legacy)?.is_none());
    Ok(())
}

#[test]
fn receipt_outside_the_old_bundles_folder_is_not_followed() -> anyhow::Result<()> {
    let moved = moved()?;
    let elsewhere = moved.root.path().join("Elsewhere/Herdr.app");
    assert!(moved.locate(&elsewhere)?.is_none());
    Ok(())
}

#[test]
fn renamed_bundle_never_claims_a_receipt_for_the_legacy_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let prefix = root.path().join("prefix");
    let legacy = root.path().join("Herdr.app");
    let copy = root.path().join("Sheprd.app");
    fs::create_dir_all(&copy)?;
    caskroom(&prefix, "20261110.1", &legacy)?;
    let uid = fs::metadata(root.path())?.uid();
    assert!(locate(&copy, uid, root.path().into(), [prefix]).is_none());
    Ok(())
}

#[test]
fn receipt_blocks_a_self_move_even_when_homebrew_is_untrusted() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let prefix = root.path().join("prefix");
    let bundle = root.path().join("Herdr.app");
    fs::create_dir_all(&bundle)?;
    caskroom(&prefix, "20261019.1", &bundle)?;
    fs::set_permissions(&prefix, fs::Permissions::from_mode(0o777))?;
    let uid = fs::metadata(root.path())?.uid();
    assert!(locate(&bundle, uid, root.path().into(), [prefix.clone()]).is_none());
    assert!(recorded_in(&bundle, [prefix]));
    assert!(!recorded_in(
        &root.path().join("Other.app"),
        [root.path().join("none")]
    ));
    Ok(())
}

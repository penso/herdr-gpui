//! Moving a standalone `Herdr.app` to `Sheprd.app` as part of an update.
use super::*;
use std::os::unix::fs::symlink;

struct Fixture {
    _root: TempDir,
    destination: PathBuf,
    target: PathBuf,
    candidate: PathBuf,
    backup: PathBuf,
}

fn fixture() -> anyhow::Result<Fixture> {
    let root = tempfile::tempdir()?;
    let applications = root.path().join("Applications");
    let stage = root.path().join("stage");
    let destination = applications.join("Herdr.app");
    let candidate = stage.join("tree/Sheprd.app");
    fs::create_dir_all(destination.join("Contents"))?;
    fs::create_dir_all(candidate.join("Contents"))?;
    fs::write(destination.join("Contents/version"), b"old")?;
    fs::write(candidate.join("Contents/version"), b"new")?;
    Ok(Fixture {
        target: applications.join("Sheprd.app"),
        backup: stage.join("previous-installation"),
        _root: root,
        destination,
        candidate,
    })
}

impl Fixture {
    fn replace(&self, launch: impl FnOnce() -> Result<()>) -> Result<()> {
        replace(
            &self.destination,
            &self.target,
            &self.candidate,
            &self.backup,
            Mode::Mac,
            launch,
        )
    }

    fn assert_restored(&self) -> anyhow::Result<()> {
        assert_eq!(fs::read(self.destination.join("Contents/version"))?, b"old");
        assert!(fs::symlink_metadata(&self.destination)?.is_dir());
        assert_eq!(fs::read(self.candidate.join("Contents/version"))?, b"new");
        assert!(fs::symlink_metadata(&self.target).is_err());
        assert!(fs::symlink_metadata(&self.backup).is_err());
        Ok(())
    }
}

#[test]
fn standalone_legacy_bundle_moves_for_a_renamed_payload() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let chosen = target(&fixture.destination, &fixture.candidate, Mode::Mac, |_| {
        false
    })?;
    assert_eq!(chosen, fixture.target);
    Ok(())
}

#[test]
fn bundle_stays_put_unless_a_move_is_safe_and_asked_for() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let stays = |destination: &Path, candidate: &Path, mode, recorded| {
        target(destination, candidate, mode, |_| recorded).map(|chosen| chosen == destination)
    };
    assert!(
        stays(&fixture.destination, &fixture.candidate, Mode::Mac, true)?,
        "Homebrew's receipt names this path"
    );
    assert!(
        stays(&fixture.destination, &fixture.candidate, Mode::Linux, false)?,
        "only macOS bundles have a name to move to"
    );
    assert!(
        stays(
            &fixture.destination,
            &fixture.candidate.with_file_name("Herdr.app"),
            Mode::Mac,
            false
        )?,
        "a legacy payload keeps the legacy name"
    );
    assert!(
        stays(&fixture.target, &fixture.candidate, Mode::Mac, false)?,
        "already moved"
    );
    Ok(())
}

#[test]
fn existing_entry_at_the_new_name_is_never_touched() -> anyhow::Result<()> {
    let fixture = fixture()?;
    fs::create_dir_all(&fixture.target)?;
    let chosen = target(&fixture.destination, &fixture.candidate, Mode::Mac, |_| {
        false
    })?;
    assert_eq!(chosen, fixture.destination, "a copy the user put there");
    fs::remove_dir(&fixture.target)?;
    symlink("missing", &fixture.target)?;
    let chosen = target(&fixture.destination, &fixture.candidate, Mode::Mac, |_| {
        false
    })?;
    assert_eq!(chosen, fixture.destination, "even a dangling link");
    Ok(())
}

#[test]
fn receipts_are_consulted_for_the_old_path_only_when_a_move_is_possible() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let mut asked = None;
    target(
        &fixture.destination,
        &fixture.candidate,
        Mode::Mac,
        |path| {
            asked = Some(path.to_owned());
            false
        },
    )?;
    assert_eq!(asked.as_ref(), Some(&fixture.destination));
    target(&fixture.target, &fixture.candidate, Mode::Mac, |_| {
        panic!("no move to ask about")
    })?;
    Ok(())
}

#[test]
fn move_installs_the_new_name_and_keeps_the_old_path_working() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let mut linked_at_launch = false;
    fixture.replace(|| {
        // A relaunched app must find the old path already resolving.
        linked_at_launch = fixture.destination.join("Contents/version").exists();
        Ok(())
    })?;
    assert!(linked_at_launch);
    assert_eq!(fs::read(fixture.target.join("Contents/version"))?, b"new");
    assert_eq!(
        fs::read_link(&fixture.destination)?,
        Path::new("Sheprd.app")
    );
    assert_eq!(
        fs::read(fixture.destination.join("Contents/version"))?,
        b"new"
    );
    assert_eq!(fs::read(fixture.backup.join("Contents/version"))?, b"old");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn legacy_link_is_hidden_from_finder_without_hiding_the_bundle() -> anyhow::Result<()> {
    use std::os::macos::fs::MetadataExt as _;
    const UF_HIDDEN: u32 = 0x8000;
    let fixture = fixture()?;
    fixture.replace(|| Ok(()))?;
    let flags = |path: &Path| fs::symlink_metadata(path).map(|meta| meta.st_flags());
    assert_ne!(flags(&fixture.destination)? & UF_HIDDEN, 0);
    assert_eq!(flags(&fixture.target)? & UF_HIDDEN, 0);
    Ok(())
}

#[test]
fn failed_launch_restores_the_old_name_and_removes_the_link() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let result = fixture.replace(|| Err(io(std::io::Error::other("spawn failed"))));
    assert!(matches!(result, Err(Error::Io(_))));
    fixture.assert_restored()
}

#[test]
fn rollback_tolerates_a_link_already_gone() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let result = fixture.replace(|| {
        fs::remove_file(&fixture.destination).map_err(io)?;
        Err(io(std::io::Error::other("spawn failed")))
    });
    assert!(matches!(result, Err(Error::Io(_))));
    fixture.assert_restored()
}

#[test]
fn rollback_never_removes_what_replaced_the_link() -> anyhow::Result<()> {
    let fixture = fixture()?;
    let result = fixture.replace(|| {
        fs::remove_file(&fixture.destination).map_err(io)?;
        fs::create_dir(&fixture.destination).map_err(io)?;
        Err(io(std::io::Error::other("spawn failed")))
    });
    assert!(matches!(result, Err(Error::RestartRecovery { .. })));
    assert!(fs::symlink_metadata(&fixture.destination)?.is_dir());
    assert_eq!(fs::read(fixture.target.join("Contents/version"))?, b"new");
    assert_eq!(fs::read(fixture.backup.join("Contents/version"))?, b"old");
    Ok(())
}

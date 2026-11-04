use super::*;
use sha2::{Digest, Sha256};
use std::{cell::Cell, os::unix::fs::symlink};

const PAYLOAD: &[u8] = b"authenticated release package";

mod migration;

fn asset() -> release::Asset {
    release::Asset {
        target: "x86_64-unknown-linux-gnu".into(),
        name: "Herdr-20261010.2-x86_64-unknown-linux-gnu.deb".into(),
        size: PAYLOAD.len() as u64,
        sha256: Sha256::digest(PAYLOAD)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn source_stage() -> Result<TempDir> {
    let stage = tempfile::tempdir()?;
    fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o700))?;
    let mut source = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(stage.path().join(asset().name))?;
    source.write_all(PAYLOAD)?;
    Ok(stage)
}

#[test]
fn replacing_a_previously_checked_package_never_reaches_the_installer() -> Result<()> {
    let source = source_stage()?;
    let storage = tempfile::tempdir()?;
    let asset = asset();
    let path = source.path().join(&asset.name);
    release::verify_archive(&path, &asset, &AtomicBool::new(false))?;
    // Represents replacement while the PolicyKit prompt was open.
    fs::write(&path, vec![b'x'; PAYLOAD.len()])?;
    let stage = open_stage(source.path(), rustix::process::geteuid().as_raw())?;
    let source = regular_file(&stage, &asset.name, rustix::process::geteuid().as_raw())?;
    let installed = Cell::new(false);
    let result = with_protected_package(source, &asset, storage.path(), |_| {
        installed.set(true);
        Ok(())
    });
    assert!(matches!(result, Err(Error::ArchiveDigest)));
    assert!(!installed.get());
    assert_eq!(
        fs::read_dir(storage.path())?.count(),
        0,
        "failed protected staging is removed"
    );
    Ok(())
}

#[test]
fn the_installer_reads_only_the_private_copy_after_source_mutation() -> Result<()> {
    let source = source_stage()?;
    let storage = tempfile::tempdir()?;
    let asset = asset();
    let original = source.path().join(&asset.name);
    let installed = Cell::new(false);
    with_protected_package(
        File::open(&original)?,
        &asset,
        storage.path(),
        |protected| {
            assert_ne!(protected, original);
            assert!(protected.starts_with(storage.path()));
            assert_eq!(fs::metadata(protected)?.mode() & 0o7777, 0o600);
            assert_eq!(
                fs::metadata(protected.parent().ok_or(Error::MissingInstallationParent)?)?.mode()
                    & 0o7777,
                0o700
            );
            assert_eq!(
                fs::metadata(protected)?.uid(),
                rustix::process::geteuid().as_raw()
            );
            fs::write(&original, b"unverified replacement")?;
            assert_eq!(fs::read(protected)?, PAYLOAD);
            release::verify_archive(protected, &asset, &AtomicBool::new(false))?;
            installed.set(true);
            Ok(())
        },
    )?;
    assert!(installed.get());
    assert_eq!(fs::read_dir(storage.path())?.count(), 0);
    Ok(())
}

#[test]
fn truncated_oversized_and_mixed_bytes_never_run_the_installer() -> Result<()> {
    let storage = tempfile::tempdir()?;
    for bytes in [
        PAYLOAD[..5].to_vec(),
        [PAYLOAD, b"extra"].concat(),
        [b"bad".as_slice(), &PAYLOAD[3..]].concat(),
    ] {
        let result = with_protected_package(bytes.as_slice(), &asset(), storage.path(), |_| {
            panic!("unauthenticated data reached the installer")
        });
        assert!(matches!(
            result,
            Err(Error::ArchiveDigest | Error::ArchiveExceedsSize)
        ));
    }
    assert_eq!(fs::read_dir(storage.path())?.count(), 0);
    Ok(())
}

#[test]
fn source_descriptors_stay_pinned_when_user_paths_are_replaced() -> Result<()> {
    let source = source_stage()?;
    let storage = tempfile::tempdir()?;
    let uid = rustix::process::geteuid().as_raw();
    let directory = open_stage(source.path(), uid)?;
    let opened = regular_file(&directory, &asset().name, uid)?;
    fs::rename(
        source.path().join(&asset().name),
        source.path().join("previous"),
    )?;
    fs::write(source.path().join(&asset().name), b"replacement")?;
    with_protected_package(opened, &asset(), storage.path(), |protected| {
        assert_eq!(fs::read(protected)?, PAYLOAD);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn symlinks_fifos_and_shared_staging_are_rejected_without_reads() -> Result<()> {
    let stage = source_stage()?;
    let uid = rustix::process::geteuid().as_raw();
    let directory = open_stage(stage.path(), uid)?;
    symlink(&asset().name, stage.path().join("link"))?;
    assert!(matches!(
        regular_file(&directory, "link", uid),
        Err(Error::Io(_))
    ));
    rustix::fs::mknodat(
        &directory,
        "fifo",
        rustix::fs::FileType::Fifo,
        Mode::RUSR | Mode::WUSR,
        0,
    )
    .map_err(std::io::Error::from)?;
    assert!(matches!(
        regular_file(&directory, "fifo", uid),
        Err(Error::StagePermissions)
    ));
    let parent = tempfile::tempdir()?;
    symlink(stage.path(), parent.path().join("link"))?;
    assert!(matches!(
        open_stage(&parent.path().join("link"), uid),
        Err(Error::Io(_))
    ));
    fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o755))?;
    assert!(matches!(
        open_stage(stage.path(), uid),
        Err(Error::StagePermissions)
    ));
    Ok(())
}

#[test]
fn privileged_entry_refuses_an_unprivileged_process_before_reading() {
    if !rustix::process::geteuid().is_root() {
        assert!(matches!(
            apply_request(Path::new("/unused"), "unused"),
            Err(Error::PackageElevation)
        ));
    }
}

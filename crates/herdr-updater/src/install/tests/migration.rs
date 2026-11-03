use super::*;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn payload(
    stage: &Path,
    root: &str,
    executable: &str,
    mac: bool,
) -> anyhow::Result<release::Offer> {
    let path = stage.join("archive.tar.gz");
    let encoder = GzEncoder::new(File::create(&path)?, Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    let mut header = tar::Header::new_ustar();
    header.set_size(3);
    header.set_mode(0o755);
    header.set_cksum();
    let entry = if mac {
        format!("{root}/Contents/MacOS/{executable}")
    } else {
        executable.into()
    };
    archive.append_data(&mut header, entry, &b"new"[..])?;
    archive.into_inner()?.finish()?;
    let bytes = fs::read(&path)?;
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let asset = release::Asset {
        target: if mac {
            "universal-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        }
        .into(),
        name: if mac {
            "sheprd-20260920.2-macos-universal.app.tar.gz".into()
        } else {
            format!("{executable}-update.tar.gz")
        },
        size: bytes.len() as u64,
        sha256: hex(&Sha256::digest(&bytes)),
    };
    let manifest = release::Manifest {
        schema: 1,
        version: "20260920.2".into(),
        assets: vec![asset.clone()],
    };
    let bytes = serde_json::to_vec(&manifest)?;
    let key = SigningKey::from_bytes(&[42; 32]);
    let signature = key.sign(&bytes).to_bytes().to_vec();
    let authenticated = release::verify_manifest(
        &bytes,
        &signature,
        &hex(key.verifying_key().as_bytes()),
        "20260920.2",
    )?;
    Ok(release::Offer {
        manifest: authenticated,
        asset,
        manifest_bytes: bytes,
        signature,
        notes: String::new(),
    })
}

#[test]
fn future_bundle_extracts_relaunches_new_executable_in_place_and_recovers() -> anyhow::Result<()> {
    for fail in [false, true] {
        let root = tempfile::tempdir()?;
        let destination = root.path().join("Herdr.app");
        fs::create_dir_all(destination.join("Contents/MacOS"))?;
        fs::write(destination.join("Contents/MacOS/Herdr"), b"old")?;
        let stage = private_directory(root.path())?;
        let offer = payload(stage.path(), "Sheprd.app", "sheprd", true)?;
        let installation = Installation {
            mode: Mode::Mac,
            executable: destination.join("Contents/MacOS/Herdr"),
            destination: destination.clone(),
            uid: fs::metadata(root.path())?.uid(),
        };
        let mut checked = Vec::new();
        let (_tree, candidate) = candidate_with(
            stage.path(),
            &installation,
            &offer,
            &AtomicBool::new(false),
            |path, version| {
                checked.push((path.to_owned(), version.to_owned()));
                // Only the native codesign boundary is mocked; both bundles must
                // retain the same existing bundle ID and Developer ID team.
                Ok(("TEAM123".into(), crate::APP_ID.into()))
            },
        )?;
        assert_eq!(
            checked,
            [
                (destination.clone(), crate::APP_VERSION.into()),
                (candidate.clone(), "20260920.2".into())
            ]
        );
        let executable = apply::replacement_executable(&destination, &candidate, Mode::Mac)?;
        assert_eq!(executable, destination.join("Contents/MacOS/sheprd"));
        let backup = stage.path().join("previous-installation");
        let result = replace(&destination, &candidate, &backup, Mode::Mac, || {
            assert_eq!(fs::read(&executable)?, b"new");
            assert_eq!(mac_bundle(&executable)?, destination);
            if fail {
                Err(Error::Io(std::io::Error::other("spawn failed")))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_err(), fail);
        if fail {
            assert_eq!(fs::read(destination.join("Contents/MacOS/Herdr"))?, b"old");
            assert_eq!(fs::read(candidate.join("Contents/MacOS/sheprd"))?, b"new");
        } else {
            assert_eq!(fs::read(&executable)?, b"new");
            assert_eq!(fs::read(backup.join("Contents/MacOS/Herdr"))?, b"old");
        }
    }
    Ok(())
}

#[test]
fn changed_team_failed_validation_and_cancel_leave_old_bundle_usable() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("Herdr.app");
    fs::create_dir_all(&destination)?;
    fs::write(destination.join("old"), b"old")?;
    let stage = private_directory(root.path())?;
    let offer = payload(stage.path(), "Sheprd.app", "sheprd", true)?;
    let installation = Installation {
        mode: Mode::Mac,
        executable: destination.join("Contents/MacOS/Herdr"),
        destination: destination.clone(),
        uid: fs::metadata(root.path())?.uid(),
    };
    for fail in [false, true] {
        let result = candidate_with(
            stage.path(),
            &installation,
            &offer,
            &AtomicBool::new(false),
            |path, _| {
                if path != destination && fail {
                    return Err(Error::BundleVersion);
                }
                Ok((
                    if path == destination {
                        "TEAM123"
                    } else {
                        "OTHER"
                    }
                    .into(),
                    crate::APP_ID.into(),
                ))
            },
        );
        if fail {
            assert!(matches!(result, Err(Error::BundleVersion)));
        } else {
            assert!(matches!(result, Err(Error::SigningIdentityChanged)));
        }
        assert_eq!(fs::read(destination.join("old"))?, b"old");
        assert_eq!(
            fs::read_dir(stage.path())?.count(),
            1,
            "failed extracted tree is cleaned"
        );
    }
    assert!(matches!(
        candidate(stage.path(), &installation, &offer, &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
    fs::write(stage.path().join("archive.tar.gz"), b"tampered")?;
    assert!(
        candidate_with(
            stage.path(),
            &installation,
            &offer,
            &AtomicBool::new(false),
            |_, _| { panic!("digest failure must precede native validation") }
        )
        .is_err()
    );
    assert!(matches!(
        signing_identity("Identifier=org.sheprd.app\nTeamIdentifier=TEAM123\n"),
        Err(Error::BundleIdentifier)
    ));
    Ok(())
}

#[test]
fn both_linux_names_replace_the_existing_path_and_rollback() -> anyhow::Result<()> {
    for prefix in ["herdr-gpui", "sheprd"] {
        for fail in [false, true] {
            let root = tempfile::tempdir()?;
            let destination = root.path().join("herdr-gpui");
            fs::write(&destination, b"old")?;
            let stage = private_directory(root.path())?;
            let offer = payload(
                stage.path(),
                "",
                &format!("{prefix}-20260920.2-x86_64-unknown-linux-gnu"),
                false,
            )?;
            let installation = Installation {
                mode: Mode::Linux,
                executable: destination.clone(),
                destination: destination.clone(),
                uid: fs::metadata(root.path())?.uid(),
            };
            let (_tree, candidate) =
                candidate(stage.path(), &installation, &offer, &AtomicBool::new(false))?;
            let launch = apply::replacement_executable(&destination, &candidate, Mode::Linux)?;
            let backup = stage.path().join("previous-installation");
            let result = replace(&destination, &candidate, &backup, Mode::Linux, || {
                assert_eq!(fs::read(&launch)?, b"new");
                if fail {
                    Err(Error::Io(std::io::Error::other("spawn failed")))
                } else {
                    Ok(())
                }
            });
            assert_eq!(result.is_err(), fail);
            assert_eq!(fs::read(destination)?, if fail { b"old" } else { b"new" });
        }
    }
    Ok(())
}

#[test]
fn bundle_layout_rejects_ambiguous_executables_links_and_lookalikes() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    for name in ["Herdr.app", "Sheprd.app"] {
        for exe in ["Herdr", "sheprd"] {
            let bundle = root.path().join(name);
            let path = bundle.join("Contents/MacOS").join(exe);
            fs::create_dir_all(path.parent().ok_or(Error::MissingExecutableParent)?)?;
            fs::write(&path, b"binary")?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
            assert_eq!(mac_bundle(&path)?, bundle);
            assert_eq!(bundle::executable(&bundle)?, path);
            let other =
                bundle
                    .join("Contents/MacOS")
                    .join(if exe == "Herdr" { "sheprd" } else { "Herdr" });
            fs::copy(&path, &other)?;
            assert!(bundle::executable(&bundle).is_err());
            fs::remove_file(&other)?;
            std::os::unix::fs::symlink(&path, &other)?;
            assert!(bundle::executable(&bundle).is_err());
            fs::remove_file(&other)?;
            fs::remove_file(path)?;
        }
    }
    for path in [
        "Sheprd.app.evil/Contents/MacOS/sheprd",
        "Other.app/Contents/MacOS/sheprd",
        "Sheprd.app/Contents/Other/sheprd",
        "Sheprd.app/Contents/MacOS/evil",
    ] {
        assert!(mac_bundle(&root.path().join(path)).is_err());
    }
    Ok(())
}

#[test]
fn renamed_archives_still_reject_mixed_roots_and_escaping_links() -> anyhow::Result<()> {
    for entries in [
        vec![
            ("Sheprd.app/file", b'0', "new"),
            ("Herdr.app/file", b'0', "old"),
        ],
        vec![("Sheprd.app.evil/file", b'0', "no")],
        vec![("Sheprd.app/link", b'2', "../escape")],
        vec![("Sheprd.app/file", b'1', "other")],
    ] {
        let root = tempfile::tempdir()?;
        let source = archives::archive(root.path(), &entries)?;
        let out = private_directory(root.path())?;
        assert!(
            extract(
                &source,
                out.path(),
                Mode::Mac,
                "unused",
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    Ok(())
}

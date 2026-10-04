use super::*;

#[test]
fn linux_exact_payload_and_cancel() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let source = archive(root.path(), &[("herdr-gpui-test-target", b'0', "binary")])?;
    let out = private_directory(root.path())?;
    let cancel = AtomicBool::new(false);
    let binary = extract(
        &source,
        out.path(),
        Mode::Linux,
        "herdr-gpui-test-target",
        &cancel,
    )?;
    assert_eq!(fs::read(&binary)?, b"binary");
    assert_eq!(fs::metadata(&binary)?.mode() & 0o7777, 0o755);
    assert_eq!(fs::metadata(out.path())?.mode() & 0o7777, 0o700);
    let out = private_directory(root.path())?;
    assert!(extract(&source, out.path(), Mode::Linux, "wrong", &cancel).is_err());
    cancel.store(true, Ordering::Relaxed);
    assert!(
        extract(
            &source,
            out.path(),
            Mode::Linux,
            "herdr-gpui-test-target",
            &cancel
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn mac_links_and_forbidden_entries() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let cancel = AtomicBool::new(false);
    let source = archive(
        root.path(),
        &[
            ("Herdr.app/file", b'0', "ok"),
            ("Herdr.app/link", b'2', "file"),
        ],
    )?;
    let out = private_directory(root.path())?;
    extract(&source, out.path(), Mode::Mac, "unused", &cancel)?;
    assert_eq!(fs::read(out.path().join("Herdr.app/link"))?, b"ok");
    for entries in [
        vec![("Herdr.app/link", b'2', "../../escape")],
        vec![("Herdr.app/file", b'1', "other")],
        vec![("Herdr.app/fifo", b'6', "")],
        vec![("Herdr.app/device", b'3', "")],
        vec![("Herdr.app/file", b'0', "a"), ("Herdr.app/file", b'0', "b")],
        vec![
            ("Herdr.app/link", b'2', "dir"),
            ("Herdr.app/link/file", b'0', "no"),
        ],
        vec![("other/file", b'0', "no")],
    ] {
        let source = archive(root.path(), &entries)?;
        let out = private_directory(root.path())?;
        assert!(
            extract(&source, out.path(), Mode::Mac, "unused", &cancel).is_err(),
            "{entries:?}"
        );
    }
    Ok(())
}

#[test]
fn traversal_and_link_policy() {
    for path in ["/absolute", "../escape", "Herdr.app/../escape", ""] {
        assert!(!safe_path(Path::new(path)));
    }
    assert!(safe_link(
        Path::new("Herdr.app/dir/link"),
        Path::new("../file")
    ));
    assert!(!safe_link(
        Path::new("Herdr.app/link"),
        Path::new("../outside")
    ));
}

#[test]
fn raw_malformed_archives_fail_before_payload_writes() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let cancel = AtomicBool::new(false);
    for (name, size) in [
        ("../escape", 0),
        ("/absolute", 0),
        ("Herdr.app/large", LIMIT + 1),
    ] {
        let mut header = tar::Header::new_ustar();
        header.set_mode(0o700);
        header.set_size(size);
        header.set_entry_type(tar::EntryType::Regular);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        let source = root.path().join("bad.tar.gz");
        let mut encoder = GzEncoder::new(File::create(&source)?, Compression::fast());
        encoder.write_all(header.as_bytes())?;
        encoder.finish()?;
        let out = private_directory(root.path())?;
        assert!(extract(&source, out.path(), Mode::Mac, "unused", &cancel).is_err());
        assert_eq!(fs::read_dir(out.path())?.count(), 0);
    }
    Ok(())
}

#[test]
fn digest_is_checked_before_extraction_and_staging_is_private() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let stage = private_directory(root.path())?;
    assert_eq!(fs::metadata(stage.path())?.mode() & 0o777, 0o700);
    fs::write(stage.path().join("archive.tar.gz"), b"bad")?;
    let installation = Installation {
        mode: Mode::Linux,
        destination: root.path().join("app"),
        executable: root.path().join("app"),
        uid: fs::metadata(stage.path())?.uid(),
    };
    let asset = release::Asset {
        target: "x86_64-unknown-linux-gnu".into(),
        name: "unused".into(),
        size: 3,
        sha256: "00".repeat(32),
    };
    let offer = release::Offer {
        manifest: release::Manifest {
            schema: 1,
            version: "20260920.2".into(),
            assets: vec![asset.clone()],
        },
        asset,
        manifest_bytes: vec![],
        signature: vec![],
    };
    assert!(candidate(stage.path(), &installation, &offer, &AtomicBool::new(false)).is_err());
    assert_eq!(fs::read_dir(stage.path())?.count(), 1);
    Ok(())
}

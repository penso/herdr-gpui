use super::*;

#[test]
fn bundle_distribution_permissions_are_shared_but_staging_stays_private() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("permissions.tar.gz");
    let encoder = GzEncoder::new(File::create(&source)?, Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    for (path, kind, mode, bytes) in [
        (
            "Herdr.app",
            tar::EntryType::Directory,
            0o7777,
            b"".as_slice(),
        ),
        (
            "Herdr.app/empty",
            tar::EntryType::Directory,
            0o700,
            b"".as_slice(),
        ),
        (
            "Herdr.app/Contents/MacOS/Herdr",
            tar::EntryType::Regular,
            0o6777,
            b"executable bytes".as_slice(),
        ),
        (
            "Herdr.app/Contents/Resources/config",
            tar::EntryType::Regular,
            0o6666,
            b"resource bytes".as_slice(),
        ),
        (
            "Herdr.app/Contents/Resources/current",
            tar::EntryType::Symlink,
            0o777,
            b"".as_slice(),
        ),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(kind);
        header.set_mode(mode);
        header.set_size(bytes.len() as u64);
        if kind.is_symlink() {
            header.set_link_name("config")?;
        }
        header.set_cksum();
        archive.append_data(&mut header, path, bytes)?;
    }
    archive.into_inner()?.finish()?;
    let stage = private_directory(root.path())?;
    let tree = private_directory(stage.path())?;
    let candidate = extract(
        &source,
        tree.path(),
        Mode::Mac,
        "unused",
        &AtomicBool::new(false),
    )?;
    let installed = root.path().join("Installed.app");
    fs::rename(candidate, &installed)?;
    for path in [
        "",
        "empty",
        "Contents",
        "Contents/MacOS",
        "Contents/Resources",
    ] {
        assert_eq!(
            fs::metadata(installed.join(path))?.mode() & 0o7777,
            0o755,
            "{path}"
        );
    }
    let executable = installed.join("Contents/MacOS/Herdr");
    let resource = installed.join("Contents/Resources/config");
    assert_eq!(fs::metadata(&executable)?.mode() & 0o7777, 0o755);
    assert_eq!(fs::metadata(&resource)?.mode() & 0o7777, 0o644);
    assert_eq!(fs::read(executable)?, b"executable bytes");
    assert_eq!(fs::read(resource)?, b"resource bytes");
    assert_eq!(
        fs::read(installed.join("Contents/Resources/current"))?,
        b"resource bytes"
    );
    for private in [stage.path(), tree.path()] {
        assert_eq!(fs::metadata(private)?.mode() & 0o7777, 0o700);
    }
    Ok(())
}

#[test]
fn system_package_locations_are_package_managed() {
    for managed in [
        "/usr/bin/herdr-gpui",
        "/usr/lib/herdr-gpui/herdr-gpui",
        "/nix/store/abc-herdr-gpui/bin/herdr-gpui",
    ] {
        assert!(system_managed(Path::new(managed)), "{managed}");
        assert!(
            matches!(
                linux_location(Path::new(managed), Path::new("/home/user"), 1000, false),
                Err(Error::PackageManaged)
            ),
            "{managed}"
        );
    }
    for unmanaged in [
        "/usr/local/bin/herdr-gpui",
        "/home/user/.local/bin/herdr-gpui",
        "/opt/herdr/bin/herdr-gpui",
        "/usrlocal/herdr-gpui",
    ] {
        assert!(!system_managed(Path::new(unmanaged)), "{unmanaged}");
    }
}

#[test]
fn linux_location_rejects_managed_unsafe_and_outside_home() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().canonicalize()?;
    let bin = home.join("bin");
    fs::create_dir(&bin)?;
    let executable = bin.join("herdr");
    fs::write(&executable, b"binary")?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    let uid = fs::metadata(&executable)?.uid();
    assert!(linux_location(&executable, &home, uid, false).is_ok());
    assert!(linux_location(&executable, &home, uid, true).is_err());
    assert!(linux_location(&executable, &bin.join("elsewhere"), uid, false).is_err());
    assert!(linux_location(&executable, &home, uid.wrapping_add(1), false).is_err());
    for mode in [0o600, 0o500, 0o4700, 0o2700, 0o722] {
        fs::set_permissions(&executable, fs::Permissions::from_mode(mode))?;
        assert!(
            linux_location(&executable, &home, uid, false).is_err(),
            "{mode:o}"
        );
    }
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o777))?;
    assert!(linux_location(&executable, &home, uid, false).is_err());
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o700))?;
    let link = bin.join("link");
    std::os::unix::fs::symlink(&executable, &link)?;
    assert!(linux_location(&link, &home, uid, false).is_err());
    // A launcher symlink does not disqualify its safe resolved origin.
    assert!(linux_location(&link.canonicalize()?, &home, uid, false).is_ok());
    let parent_link = home.join("linked-bin");
    std::os::unix::fs::symlink(&bin, &parent_link)?;
    assert!(linux_location(&parent_link.join("herdr"), &home, uid, false).is_err());
    let hard_link = bin.join("hard-link");
    fs::hard_link(&executable, &hard_link)?;
    assert!(linux_location(&executable, &home, uid, false).is_err());
    fs::remove_file(hard_link)?;
    let elsewhere = tempfile::tempdir()?;
    assert!(linux_location(&executable, &elsewhere.path().canonicalize()?, uid, false).is_err());
    fs::set_permissions(&home, fs::Permissions::from_mode(0o777))?;
    assert!(linux_location(&executable, &home, uid, false).is_err());
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[test]
fn mac_parent_policy_allows_root_admin_but_not_other_users_or_symlinks() -> anyhow::Result<()> {
    assert!(trusted_mac_parent(0, 0o40775, 501));
    assert!(trusted_mac_parent(501, 0o40775, 501));
    assert!(!trusted_mac_parent(502, 0o40775, 501));
    assert!(!trusted_mac_parent(0, 0o40777, 501));
    let root = tempfile::tempdir()?;
    let parent = root.path().canonicalize()?;
    let uid = fs::metadata(&parent)?.uid();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o775))?;
    assert!(installation_parent(&parent, Mode::Mac, uid).is_ok());
    assert!(installation_parent(&parent, Mode::Linux, uid).is_err());
    let stage = private_directory(&parent)?;
    assert_eq!(fs::metadata(stage.path())?.mode() & 0o777, 0o700);
    drop(stage);
    let link = parent.join("linked-parent");
    std::os::unix::fs::symlink(&parent, &link)?;
    assert!(installation_parent(&link, Mode::Mac, uid).is_err());
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o777))?;
    assert!(installation_parent(&parent, Mode::Mac, uid).is_err());
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555))?;
    if uid != 0 {
        assert!(private_directory(&parent).is_err());
    }
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[test]
fn shared_mac_parent_does_not_relax_lock_file_policy() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let parent = root.path().canonicalize()?;
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o775))?;
    let uid = fs::metadata(&parent)?.uid();
    let installation = Installation {
        mode: Mode::Mac,
        destination: parent.join("Herdr.app"),
        executable: parent.join("unused"),
        uid,
    };
    let path = parent.join(".Herdr.app.update-lock");
    let sentinel = parent.join("sentinel");
    fs::write(&sentinel, b"do not modify")?;
    fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o600))?;
    std::os::unix::fs::symlink(&sentinel, &path)?;
    assert!(lock_file(&installation, ".update-lock").is_err());
    fs::remove_file(&path)?;
    fs::hard_link(&sentinel, &path)?;
    assert!(lock_file(&installation, ".update-lock").is_err());
    assert!(owned(&path, uid.wrapping_add(1), false).is_err());
    fs::remove_file(&path)?;
    fs::write(&path, b"unsafe lock")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666))?;
    assert!(lock_file(&installation, ".update-lock").is_err());
    assert_eq!(fs::read(&sentinel)?, b"do not modify");
    Ok(())
}

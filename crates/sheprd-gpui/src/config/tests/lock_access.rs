use super::*;

#[test]
fn maintenance_lock_can_be_opened_but_not_locked_by_another_writer() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let lock_path = path.with_extension("lock");
    let (first, _) = Config::prepare_files(&path)?;
    // Opening is shared; the byte-range/file lock serializes maintenance.
    let second = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)?;
    assert!(matches!(
        second.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(first);
    second.try_lock()?;
    drop(second);
    // Reopening the existing lock must work just like creating it did.
    let (_lock, _) = Config::prepare_files(&path)?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn read_only_lock_preserves_native_open_error_and_leaves_config_untouched() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let lock_path = path.with_extension("lock");
    let original = "theme = 'Nord'\n";
    fs::write(&path, original)?;
    fs::write(&lock_path, b"")?;
    let permissions = fs::metadata(&lock_path)?.permissions();
    let mut read_only = permissions.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&lock_path, read_only)?;
    let result = Config::prepare_files(&path);
    // Restore before asserting, so even a regression leaves a removable fixture.
    fs::set_permissions(&lock_path, permissions)?;
    let error = result.err().context("read-only lock unexpectedly opened")?;
    let crate::Error::Path {
        path: failed,
        source,
    } = &error
    else {
        panic!("missing lock-path context: {error:?}");
    };
    assert_eq!(failed, &lock_path);
    let crate::Error::Io(cause) = source.as_ref() else {
        panic!("missing native cause: {source:?}");
    };
    assert_eq!(cause.kind(), ErrorKind::PermissionDenied);
    assert_eq!(cause.raw_os_error(), Some(5));
    assert_eq!(fs::read_to_string(&path)?, original);
    assert!(!path.with_extension("local.toml").exists());
    Ok(())
}

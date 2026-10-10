use super::*;

fn load_error(path: PathBuf) -> anyhow::Result<crate::Error> {
    Settings::load_path(path)
        .err()
        .ok_or_else(|| anyhow::anyhow!("loaded a writable config path"))
}

fn permission_error(error: &crate::Error, expected_path: &Path, expected_mode: u32) {
    assert!(matches!(
        source(error),
        Some(Error::InsecurePermissions { path, mode })
            if path == expected_path && *mode == expected_mode
    ));
    let message = error.to_string();
    assert!(message.contains("writable by group or others"));
    assert!(message.contains("chmod go-w"));
    assert!(message.contains("Reload settings"));
}

#[test]
fn group_writable_config_reports_path_and_recovers_after_chmod() -> anyhow::Result<()> {
    let temp = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let path = temp.path().join("config.toml");
    let text = "# keep this comment\n[theme]\nname = 'vesper'\n";
    fs::write(&path, text)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o664))?;

    let error = load_error(path.clone())?;
    permission_error(&error, &path, 0o664);
    assert_eq!(fs::read_to_string(&path)?, text);
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o664);

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    let settings = Settings::load_path(path.clone())?;
    assert_eq!(settings.theme_name, "vesper");
    settings.save(Edit::Theme("nord".into()))?;
    assert_eq!(Settings::load_path(path.clone())?.theme_name, "nord");
    assert!(fs::read_to_string(&path)?.contains("# keep this comment"));
    assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o644);
    Ok(())
}

#[test]
fn group_writable_parent_reports_directory_then_file() -> anyhow::Result<()> {
    let temp = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let parent = temp.path().join("herdr");
    fs::create_dir(&parent)?;
    let path = parent.join("config.toml");
    fs::write(&path, "[theme]\nname = 'vesper'\n")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o664))?;
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o775))?;

    permission_error(&load_error(path.clone())?, &parent, 0o775);
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755))?;
    permission_error(&load_error(path.clone())?, &path, 0o664);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert_eq!(Settings::load_path(path)?.theme_name, "vesper");
    Ok(())
}

#[test]
fn writable_ancestor_reports_the_ancestor_path() -> anyhow::Result<()> {
    let temp = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let ancestor = temp.path().join("config");
    let parent = ancestor.join("herdr");
    fs::create_dir_all(&parent)?;
    let path = parent.join("config.toml");
    fs::write(&path, "")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&ancestor, fs::Permissions::from_mode(0o775))?;

    permission_error(&load_error(path)?, &ancestor, 0o775);
    Ok(())
}

#[test]
fn writable_lock_reports_the_lock_and_leaves_config_unchanged() -> anyhow::Result<()> {
    let temp = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let path = temp.path().join("config.toml");
    let settings = Settings::load_path(path.clone())?.save(Edit::Sound(true))?;
    let original = fs::read(&path)?;
    let lock = temp.path().join(".config.toml.gpui-lock");
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o664))?;

    permission_error(
        &settings
            .save(Edit::Sound(false))
            .err()
            .ok_or_else(|| anyhow::anyhow!("saved through writable path"))?,
        &lock,
        0o664,
    );
    assert_eq!(fs::read(path)?, original);
    Ok(())
}

#[test]
fn permissions_changed_after_load_prevent_saving() -> anyhow::Result<()> {
    let temp = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let path = temp.path().join("config.toml");
    let settings = Settings::load_path(path.clone())?.save(Edit::Sound(true))?;
    let original = fs::read(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o664))?;

    permission_error(
        &settings
            .save(Edit::Sound(false))
            .err()
            .ok_or_else(|| anyhow::anyhow!("saved through writable path"))?,
        &path,
        0o664,
    );
    assert_eq!(fs::read(path)?, original);
    Ok(())
}

use super::*;
use std::os::unix::fs::symlink;

#[test]
fn dotfile_links_are_read_and_saved_through_without_replacing_the_link() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let store = temp.path().join("dotfiles/herdr");
    fs::create_dir_all(&store)?;
    let target = store.join("config.toml");
    fs::write(&target, "# kept\n[ui.sound]\nenabled = true\n")?;
    let config = temp.path().join("config/herdr");
    fs::create_dir_all(&config)?;
    let path = config.join("config.toml");
    symlink("../../dotfiles/herdr/config.toml", &path)?;

    let settings = Settings::load_path(path.clone())?;
    assert!(settings.sound_enabled);
    assert!(!settings.save(Edit::Sound(false))?.sound_enabled);
    assert_eq!(
        fs::read_link(&path)?,
        Path::new("../../dotfiles/herdr/config.toml")
    );
    assert_eq!(
        fs::read_to_string(&target)?,
        "# kept\n[ui.sound]\nenabled = false\n"
    );
    // The lock and temporary file belong beside the target, not the link.
    assert_eq!(fs::read_dir(&config)?.count(), 1);
    Ok(())
}

#[test]
fn linked_config_directories_are_followed() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let store = temp.path().join("dotfiles/herdr");
    fs::create_dir_all(&store)?;
    fs::write(store.join("config.toml"), "[ui.sound]\nenabled = false\n")?;
    let config = temp.path().join("config");
    fs::create_dir(&config)?;
    symlink(&store, config.join("herdr"))?;
    let path = config.join("herdr/config.toml");

    let settings = Settings::load_path(path.clone())?;
    assert!(!settings.sound_enabled);
    assert!(settings.save(Edit::Sound(true))?.sound_enabled);
    assert!(
        fs::symlink_metadata(config.join("herdr"))?
            .file_type()
            .is_symlink()
    );
    assert!(Settings::load_path(store.join("config.toml"))?.sound_enabled);
    Ok(())
}

#[test]
fn parent_steps_after_a_link_resolve_like_the_kernel() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    fs::create_dir_all(temp.path().join("a/b"))?;
    fs::write(
        temp.path().join("a/config.toml"),
        "[ui.sound]\nenabled = false\n",
    )?;
    fs::write(
        temp.path().join("config.toml"),
        "[ui.sound]\nenabled = true\n",
    )?;
    symlink("a/b", temp.path().join("alias"))?;
    fs::create_dir(temp.path().join("herdr"))?;
    let path = temp.path().join("herdr/config.toml");
    // `alias/..` is `a`, not the directory holding `alias`.
    symlink("../alias/../config.toml", &path)?;

    assert_eq!(fs::read_to_string(&path)?, "[ui.sound]\nenabled = false\n");
    assert!(!Settings::load_path(path)?.sound_enabled);
    Ok(())
}

#[test]
fn links_to_the_snapshot_directory_are_followed() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let parent = temp.path().join("config");
    fs::create_dir(&parent)?;
    let path = parent.join("config.toml");
    let settings = Settings::load_path(path.clone())?.save(Edit::Sound(true))?;
    let moved = temp.path().join("moved");
    fs::rename(&parent, &moved)?;
    symlink(&moved, &parent)?;

    // Renaming keeps the directory's identity, so the snapshot still matches.
    assert!(!settings.save(Edit::Sound(false))?.sound_enabled);
    assert!(fs::symlink_metadata(&parent)?.file_type().is_symlink());
    assert!(!Settings::load_path(moved.join("config.toml"))?.sound_enabled);
    Ok(())
}

#[test]
fn a_link_created_after_an_empty_load_is_followed() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let target = temp.path().join("target");
    fs::create_dir(&target)?;
    let alias = temp.path().join("alias");
    let settings = Settings::load_path(alias.join("new/config.toml"))?;
    symlink(&target, &alias)?;

    settings.save(Edit::Sound(false))?;
    assert!(!Settings::load_path(target.join("new/config.toml"))?.sound_enabled);
    Ok(())
}

#[test]
fn dangling_links_are_refused() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let target = temp.path().join("dotfiles/config.toml");
    let path = temp.path().join("config.toml");
    symlink(&target, &path)?;

    let error = Settings::load_path(path)
        .err()
        .ok_or_else(|| anyhow::anyhow!("loaded through a dangling link"))?;
    assert!(matches!(source(&error), Some(Error::UnsafePath)));
    assert!(!target.exists());
    Ok(())
}

#[test]
fn links_in_directories_others_can_write_are_refused() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let private = temp.path().join("private");
    fs::create_dir(&private)?;
    fs::write(private.join("config.toml"), "[ui.sound]\nenabled = true\n")?;
    let open = temp.path().join("open");
    fs::create_dir(&open)?;
    let path = open.join("config.toml");
    symlink(private.join("config.toml"), &path)?;
    let settings = Settings::load_path(path.clone())?;
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777))?;

    // Another user could swap this link, even though its target is private.
    assert!(matches!(
        persistence::read(&path),
        Err(Error::InsecurePermissions { .. })
    ));
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("saved through a link others can swap"))?;
    assert!(matches!(
        source(&error),
        Some(Error::InsecurePermissions { .. })
    ));
    assert!(Settings::load_path(private.join("config.toml"))?.sound_enabled);
    Ok(())
}

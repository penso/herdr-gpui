use super::*;

const ID: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";

#[test]
fn devices_set_their_own_terminal_theme() -> anyhow::Result<()> {
    let config = Config::parse(&format!(
        "theme = 'Nord'\n[devices.{ID}]\ntheme = 'Dracula'\n[devices.{OTHER}]\nkeybindings = 'server'\ntheme = 'light:Catppuccin Latte,dark:Catppuccin Mocha'"
    ))?;
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    assert_eq!(config.device_theme(&format!("ssh:{ID}")), Some("Dracula"));
    assert_eq!(
        config.device_theme(&format!("ssh:{OTHER}")),
        Some("light:Catppuccin Latte,dark:Catppuccin Mocha")
    );
    // The theme sits beside the keybindings choice without changing it.
    assert_eq!(
        config.keybinding_source(&format!("ssh:{OTHER}")),
        KeybindingSource::Server
    );
    assert_eq!(
        config.device_themes().collect::<Vec<_>>(),
        [
            (ID, "Dracula"),
            (OTHER, "light:Catppuccin Latte,dark:Catppuccin Mocha")
        ]
    );
    // Local, explicit sockets, unlisted devices, and a bare profile ID keep
    // the app theme.
    for endpoint in [
        "local",
        "socket",
        ID,
        "ssh:00000000000000000000000000000000",
    ] {
        assert_eq!(config.device_theme(endpoint), None, "{endpoint}");
    }
    // A device without a theme lists none.
    let keys_only = Config::parse(&format!("[devices.{ID}]\nkeybindings = 'server'"))?;
    assert_eq!(keys_only.device_theme(&format!("ssh:{ID}")), None);
    assert_eq!(keys_only.device_themes().count(), 0);
    Ok(())
}

#[test]
fn a_device_theme_that_cannot_parse_names_its_device() -> anyhow::Result<()> {
    for (text, inner) in [
        (
            format!("[devices.{ID}]\ntheme = '  '"),
            "theme must not be empty",
        ),
        (
            format!("[devices.{ID}]\ntheme = 'light:Nord'"),
            "a theme that follows the system must name both sides",
        ),
    ] {
        let error = Config::parse(&text).err().context("accepted")?;
        let crate::Error::Settings(Error::DeviceTheme { device, source }) = &error else {
            anyhow::bail!("{text:?} gave {error:?}");
        };
        assert_eq!(device, ID);
        assert!(source.to_string().starts_with(inner), "{source}");
        assert!(
            error
                .to_string()
                .starts_with(&format!("devices.{ID}.theme: ")),
            "{error}"
        );
        assert!(std::error::Error::source(&error).is_some());
    }
    assert!(Config::parse(&format!("[devices.{ID}]\ntheme = 5")).is_err());
    Ok(())
}

#[test]
fn device_themes_resolve_with_the_config_contrast() -> anyhow::Result<()> {
    let config = Config::parse("")?;
    let pair = "light:Catppuccin Latte,dark:Catppuccin Mocha";
    assert_eq!(
        config.theme_named(pair, true)?,
        Theme::builtin("Catppuccin Latte").context("built-in")?
    );
    assert_eq!(
        config.theme_named(pair, false)?,
        Theme::builtin("Catppuccin Mocha").context("built-in")?
    );
    let contrasted = Config::parse("contrast = 'high'")?;
    assert_eq!(
        contrasted.theme_named("Nord", false)?,
        Theme::builtin("Nord")
            .context("built-in")?
            .with_contrast(contrasted.contrast)
    );
    assert!(matches!(
        config.theme_named("../escape", false),
        Err(crate::Error::Settings(Error::InvalidThemePath))
    ));
    Ok(())
}

#[test]
fn device_theme_saves_beside_keybindings_and_removes_cleanly() -> anyhow::Result<()> {
    let endpoint = format!("ssh:{ID}");
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n[usage]\nshow = false\n";
    fs::write(&path, original)?;
    Config::save_device_theme_path(ID, Some("Dracula"), &path)?;
    let saved = fs::read_to_string(&path)?;
    assert!(saved.starts_with(original), "{saved}");
    assert!(
        saved.contains(&format!("[devices.{ID}]\ntheme = \"Dracula\"")),
        "{saved}"
    );
    let config = Config::parse(&saved)?;
    assert_eq!(config.device_theme(&endpoint), Some("Dracula"));
    assert_eq!(config.theme, "Nord");

    // Keybindings join the same table, and removing the theme keeps them.
    Config::save_device_keybindings_path(ID, KeybindingSource::Server, &path)?;
    Config::save_device_theme_path(ID, None, &path)?;
    let config = Config::parse(&fs::read_to_string(&path)?)?;
    assert_eq!(config.device_theme(&endpoint), None);
    assert_eq!(
        config.keybinding_source(&endpoint),
        KeybindingSource::Server
    );
    // With both gone, the file is as it was.
    Config::save_device_keybindings_path(ID, KeybindingSource::Local, &path)?;
    assert_eq!(fs::read_to_string(&path)?, original);
    Config::save_device_theme_path(ID, None, &path)?;
    assert_eq!(fs::read_to_string(&path)?, original);

    // A value that could never load, or an ID that is not a catalog
    // profile, never reaches the file.
    assert!(matches!(
        Config::save_device_theme_path(ID, Some(" "), &path),
        Err(crate::Error::Settings(Error::DeviceTheme { .. }))
    ));
    assert!(matches!(
        Config::save_device_theme_path("../x", Some("Nord"), &path),
        Err(crate::Error::Settings(Error::InvalidDeviceId(_)))
    ));
    assert_eq!(fs::read_to_string(&path)?, original);

    // A missing file starts from the local template.
    fs::remove_file(&path)?;
    Config::save_device_theme_path(ID, Some("Nord"), &path)?;
    let created = fs::read_to_string(&path)?;
    assert!(created.starts_with(LOCAL_CONFIG), "{created}");
    assert_eq!(
        Config::parse(&created)?.device_theme(&endpoint),
        Some("Nord")
    );
    Ok(())
}

/// `herdr-gpui config set devices.<id>.theme NAME` is how a script sets it:
/// the edit is checked like a load, so a value that could not parse is
/// refused before it is written.
#[test]
fn the_command_line_sets_and_unsets_a_device_theme() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let local = temp.0.join("config-gpui.local.toml");
    let daemon = temp.0.join("absent.toml");
    fs::write(&local, "theme = \"Nord\"\n")?;
    let key = format!("devices.{ID}.theme");
    assert!(Config::edit_key_at(
        &local,
        &daemon,
        &key,
        Some(parse_value("Dracula"))
    )?);
    let config = Config::parse(&fs::read_to_string(&local)?)?;
    assert_eq!(config.device_theme(&format!("ssh:{ID}")), Some("Dracula"));
    let before = fs::read_to_string(&local)?;
    assert!(matches!(
        Config::edit_key_at(&local, &daemon, &key, Some(parse_value("light:Nord"))),
        Err(crate::Error::Settings(Error::DeviceTheme { .. }))
    ));
    assert_eq!(fs::read_to_string(&local)?, before);
    assert!(Config::edit_key_at(&local, &daemon, &key, None)?);
    let config = Config::parse(&fs::read_to_string(&local)?)?;
    assert_eq!(config.device_theme(&format!("ssh:{ID}")), None);
    Ok(())
}

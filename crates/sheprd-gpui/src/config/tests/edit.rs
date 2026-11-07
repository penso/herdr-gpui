use super::*;

fn files() -> anyhow::Result<(TempDirectory, PathBuf, PathBuf, PathBuf)> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    let daemon = temp.0.join("absent.toml");
    fs::write(&local, "# Mine\ntheme = \"Nord\" # kept\n")?;
    Ok((temp, path, local, daemon))
}

#[test]
fn values_parse_as_toml_and_fall_back_to_plain_strings() {
    assert_eq!(parse_value("14").as_integer(), Some(14));
    assert_eq!(parse_value("1.5").as_float(), Some(1.5));
    assert_eq!(parse_value("true").as_bool(), Some(true));
    assert_eq!(parse_value("\"Nord\"").as_str(), Some("Nord"));
    assert_eq!(parse_value("Dracula").as_str(), Some("Dracula"));
    assert_eq!(
        parse_value("light:A, dark:B").as_str(),
        Some("light:A, dark:B")
    );
    assert_eq!(
        parse_value("[\"a\", \"b\"]")
            .as_array()
            .map(|array| array.len()),
        Some(2)
    );
    assert!(parse_value("{ show = true }").is_inline_table());
}

#[test]
fn setting_a_nested_key_keeps_the_rest_of_the_file() -> anyhow::Result<()> {
    let (_temp, path, local, daemon) = files()?;
    assert!(Config::edit_key_at(
        &local,
        &daemon,
        "terminal.size",
        Some(parse_value("16"))
    )?);
    let text = fs::read_to_string(&local)?;
    assert!(
        text.starts_with("# Mine\ntheme = \"Nord\" # kept\n"),
        "{text}"
    );
    assert!(text.contains("[terminal]\nsize = 16"), "{text}");
    assert_eq!(
        Config::effective_value_at(&path, "terminal.size")?,
        Some((toml::Value::Integer(16), Layer::Local))
    );
    // Replacing a value keeps its comment.
    Config::edit_key_at(&local, &daemon, "theme", Some(parse_value("Dracula")))?;
    assert!(fs::read_to_string(&local)?.contains("theme = \"Dracula\" # kept"));
    assert_eq!(Config::check_files_at(&path, &daemon)?.theme, "Dracula");
    Ok(())
}

#[test]
fn unsetting_removes_the_key_and_empty_tables() -> anyhow::Result<()> {
    let (_temp, path, local, daemon) = files()?;
    Config::edit_key_at(&local, &daemon, "terminal.size", Some(parse_value("16")))?;
    assert!(Config::edit_key_at(&local, &daemon, "terminal.size", None)?);
    let text = fs::read_to_string(&local)?;
    assert!(!text.contains("terminal"), "{text}");
    assert!(!Config::edit_key_at(
        &local,
        &daemon,
        "terminal.size",
        None
    )?);
    assert!(!Config::edit_key_at(&local, &daemon, "nothing.here", None)?);
    // The managed default shows through once the override is gone.
    let (_, layer) = Config::effective_value_at(&path, "terminal.size")?
        .ok_or_else(|| anyhow::anyhow!("terminal.size has a default"))?;
    assert_eq!(layer, Layer::Default);
    Ok(())
}

#[test]
fn invalid_values_and_unknown_keys_leave_the_file_unchanged() -> anyhow::Result<()> {
    let (_temp, _path, local, daemon) = files()?;
    let before = fs::read_to_string(&local)?;
    for (key, value) in [
        ("terminal.size", "\"large\""),
        ("terminal.size", "999"),
        ("theme", "\"\""),
        ("terminal.sise", "14"),
        ("no_such_setting", "true"),
        ("github.token", "\"secret\""),
    ] {
        assert!(
            Config::edit_key_at(&local, &daemon, key, Some(parse_value(value))).is_err(),
            "{key} = {value}"
        );
        assert_eq!(fs::read_to_string(&local)?, before, "{key} = {value}");
    }
    assert!(matches!(
        Config::edit_key_at(&local, &daemon, "terminal.sise", Some(parse_value("14"))),
        Err(crate::Error::Settings(Error::UnknownKeys(keys))) if keys == ["terminal.sise"]
    ));
    // A part of the key that holds a value cannot hold a key.
    assert!(matches!(
        Config::edit_key_at(&local, &daemon, "theme.name", Some(parse_value("x"))),
        Err(crate::Error::Settings(Error::KeyNotTable { .. }))
    ));
    assert_eq!(fs::read_to_string(&local)?, before);
    Ok(())
}

#[test]
fn an_unknown_key_already_in_the_file_does_not_block_other_edits() -> anyhow::Result<()> {
    let (_temp, path, local, daemon) = files()?;
    fs::write(&local, "old_key = 1\n")?;
    Config::edit_key_at(&local, &daemon, "show_agents", Some(parse_value("false")))?;
    let config = Config::check_files_at(&path, &daemon)?;
    assert!(!config.show_agents);
    assert_eq!(config.unknown_keys, ["old_key"]);
    Ok(())
}

#[test]
fn reading_never_writes() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    assert!(Config::effective_value_at(&path, "theme")?.is_some());
    assert_eq!(Config::effective_value_at(&path, "no.such.key")?, None);
    Config::check_files_at(&path, &temp.0.join("absent.toml"))?;
    assert_eq!(fs::read_dir(&temp.0)?.count(), 0);
    Ok(())
}

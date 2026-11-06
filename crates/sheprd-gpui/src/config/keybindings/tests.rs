use super::*;

#[test]
fn edit_unbind_and_reset_preserve_other_overrides_and_comments() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config-gpui.local.toml");
    fs::write(
        &path,
        "# personal\ntheme = 'Nord'\n[keybindings]\nnew_tab = 'alt-t' # tab\nquit = 'alt-q'\n",
    )?;
    Config::save_keybinding_path(
        "new_tab",
        Some(Binding::Many(vec!["ctrl-shift-t".into(), "alt-t".into()])),
        &path,
    )?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# personal") && text.contains("# tab"));
    let config = Config::parse(&text)?;
    assert_eq!(config.theme, "Nord");
    assert_eq!(config.keybindings.primary(crate::Command::Quit), "alt-q");
    assert_eq!(
        config
            .keybindings
            .shortcuts(crate::Command::Tab)
            .collect::<Vec<_>>(),
        ["ctrl-shift-t", "alt-t"]
    );
    Config::save_keybinding_path("new_tab", Some(Binding::Many(vec![])), &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?
            .keybindings
            .primary(crate::Command::Tab),
        ""
    );
    Config::save_keybinding_path("new_tab", None, &path)?;
    let config = Config::parse(&fs::read_to_string(&path)?)?;
    assert!(!config.keybinding_overrides.contains_key("new_tab"));
    assert_eq!(
        config.keybindings.primary(crate::Command::Tab),
        crate::controls::platform_default("cmd-t")
    );
    assert_eq!(config.keybindings.primary(crate::Command::Quit), "alt-q");
    Ok(())
}

#[test]
fn invalid_or_conflicting_edits_never_replace_the_file() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config-gpui.local.toml");
    let original = "[keybindings]\nquit = 'alt-q'\n[pane_keys]\n'ctrl-alt-l' = 'ctrl-l'\n";
    fs::write(&path, original)?;
    for key in ["t", "alt-q", "ctrl-alt-l", "ctrl-nope-t"] {
        assert!(
            Config::save_keybinding_path("new_tab", Some(Binding::One(key.into())), &path).is_err()
        );
        assert_eq!(fs::read_to_string(&path)?, original);
    }
    assert!(Config::save_keybinding_path("unknown", None, &path).is_err());
    fs::write(&path, "[broken")?;
    assert!(Config::save_keybinding_path("new_tab", None, &path).is_err());
    assert_eq!(fs::read_to_string(&path)?, "[broken");
    Ok(())
}

#[test]
fn inline_tables_and_missing_files_support_shortcut_edits() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config-gpui.local.toml");
    Config::save_keybinding_path("new_tab", Some(Binding::One("alt-t".into())), &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?
            .keybindings
            .primary(crate::Command::Tab),
        "alt-t"
    );
    fs::write(&path, "keybindings = {new_tab = 'alt-t', quit = 'alt-q'}\n")?;
    Config::save_keybinding_path("new_tab", None, &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?
            .keybindings
            .primary(crate::Command::Quit),
        "alt-q"
    );
    Ok(())
}

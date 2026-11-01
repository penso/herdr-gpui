#![allow(clippy::expect_used)]
use super::*;

#[test]
fn targeted_preferences_round_trip_and_reset_without_erasing_unknown_toml() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(
        &path,
        "# personal\nfuture = 'keep'\nlayout = 'orca' # mode\n[notifications]\nfuture = 42\nenabled = false # enabled\n",
    )?;
    for edit in [
        Preference::ConfirmCloseTab(false),
        Preference::ConfirmClosePane(false),
        Preference::ConfirmCloseWorkspace(false),
        Preference::RemoveMergedWorktrees(true),
        Preference::ShowSystemLoad(false),
        Preference::ShowNetworkQuality(true),
        Preference::AgentCheckpoints(false),
        Preference::ShowListeningPorts(false),
        Preference::ShowSidebarSearch(false),
        Preference::ShowRepositoryIcons(false),
        Preference::ShowDeviceIcons(false),
        Preference::SmoothScroll(false),
        Preference::NotificationEnabled(Some(true)),
        Preference::NotificationDelay(Some(3600)),
        Preference::NotificationPosition(Some(ToastHerdrPosition::TopLeft)),
        Preference::ClipboardEnabled(Some(false)),
        Preference::ClipboardPosition(Some(ClipboardToastPosition::TopCenter)),
        Preference::TerminalPadding(Axis::X, Sides::new(7.5, 7.5).expect("in band")),
        Preference::TerminalPadding(Axis::Y, Sides::new(4., 6.).expect("in band")),
        Preference::UpdateChannel(UpdateChannel::Beta),
        Preference::Feature(Feature::SidebarHoverMenu, true),
    ] {
        Config::save_preference_path(edit, &path)?;
    }
    let text = fs::read_to_string(&path)?;
    for comment in ["# personal", "# mode", "enabled = true # enabled"] {
        assert!(text.contains(comment));
    }
    let table: toml::Table = toml::from_str(&text)?;
    assert_eq!(table["future"].as_str(), Some("keep"));
    assert_eq!(table["notifications"]["future"].as_integer(), Some(42));
    assert_eq!(table["confirm_close_tab"].as_bool(), Some(false));
    assert_eq!(table["confirm_close_pane"].as_bool(), Some(false));
    assert_eq!(table["confirm_close_workspace"].as_bool(), Some(false));
    assert_eq!(table["remove_merged_worktrees"].as_bool(), Some(true));
    assert_eq!(table["show_system_load"].as_bool(), Some(false));
    assert_eq!(table["show_network_quality"].as_bool(), Some(true));
    assert_eq!(table["agent_checkpoints"].as_bool(), Some(false));
    assert_eq!(table["show_listening_ports"].as_bool(), Some(false));
    assert_eq!(table["show_sidebar_search"].as_bool(), Some(false));
    assert_eq!(table["show_repository_icons"].as_bool(), Some(false));
    assert_eq!(table["show_device_icons"].as_bool(), Some(false));
    assert_eq!(table["smooth_scroll"].as_bool(), Some(false));
    assert_eq!(table["layout"].as_str(), Some("orca"));
    assert_eq!(table["updates"]["channel"].as_str(), Some("beta"));
    assert_eq!(
        table["features"]["sidebar_hover_menu"].as_bool(),
        Some(true)
    );
    assert_eq!(table["terminal"]["padding_x"].as_float(), Some(7.5));
    assert_eq!(
        table["terminal"]["padding_y"],
        toml::Value::Array(vec![4.into(), 6.into()])
    );
    assert_eq!(
        table["notifications"]["delay_seconds"].as_integer(),
        Some(3600)
    );
    assert_eq!(
        table["notifications"]["position"].as_str(),
        Some("top-left")
    );
    assert_eq!(table["clipboard_toast"]["enabled"].as_bool(), Some(false));
    assert_eq!(
        table["clipboard_toast"]["position"].as_str(),
        Some("top-center")
    );
    for edit in [
        Preference::NotificationEnabled(None),
        Preference::NotificationDelay(None),
        Preference::NotificationPosition(None),
        Preference::ClipboardEnabled(None),
        Preference::ClipboardPosition(None),
    ] {
        Config::save_preference_path(edit, &path)?;
    }
    let table: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
    assert_eq!(
        table["notifications"].as_table().map(|table| table.len()),
        Some(1)
    );
    assert_eq!(
        table["clipboard_toast"].as_table().map(|table| table.len()),
        Some(0)
    );
    Ok(())
}

#[test]
fn preference_validation_is_atomic_and_preserves_typed_causes() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    let original = "layout = { mode = 'compact', sidebar_gap = 3 }\n";
    fs::write(&path, original)?;
    let edit = Preference::NotificationDelay(Some(u64::MAX));
    let error = Config::save_preference_path(edit, &path).expect_err("invalid preference");
    assert!(matches!(error, crate::Error::Path { .. }));
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(fs::read_to_string(&path)?, original);
    let sides = Sides::new(64., 0.).expect("padding in band");
    Config::save_preference_path(Preference::TerminalPadding(Axis::Y, sides), &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?
            .terminal_padding
            .y,
        sides
    );
    for original in ["notifications = false\n", "[invalid"] {
        fs::write(&path, original)?;
        assert!(
            Config::save_preference_path(Preference::NotificationEnabled(Some(true)), &path)
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path)?, original);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn preference_save_writes_through_a_symlinked_file() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let dotfiles = directory.path().join("dotfiles");
    fs::create_dir(&dotfiles)?;
    let tracked = dotfiles.join("local.toml");
    fs::write(&tracked, "# tracked\n")?;
    let link = directory.path().join("local.toml");
    std::os::unix::fs::symlink(&tracked, &link)?;

    Config::save_preference_path(Preference::ShowSystemLoad(false), &link)?;

    assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
    let text = fs::read_to_string(&tracked)?;
    assert!(text.contains("# tracked"));
    let table: toml::Table = toml::from_str(&text)?;
    assert_eq!(table["show_system_load"].as_bool(), Some(false));
    Ok(())
}

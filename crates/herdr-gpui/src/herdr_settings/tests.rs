use super::*;
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
use std::{fs, path::Path};

fn parsed(text: &str) -> Result<Settings, Error> {
    #[cfg(unix)]
    let snapshot = persistence::Snapshot {
        text: Some(text.into()),
        ..Default::default()
    };
    #[cfg(windows)]
    let snapshot = persistence::Snapshot {
        text: Some(text.into()),
    };
    Settings::parse(PathBuf::from("/fixture/herdr/config.toml"), snapshot)
}

#[cfg(windows)]
#[test]
fn windows_shared_settings_are_bounded_and_read_only() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("missing/config.toml");
    let settings = Settings::load_path(path.clone())?;
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("saved on Windows"))?;
    assert!(matches!(source(&error), Some(Error::Unsupported)));
    assert!(!path.parent().is_some_and(Path::exists));
    let path = temp.path().join("config.toml");
    let original = "[ui.sound]\nenabled = false\n";
    fs::write(&path, original)?;
    let settings = Settings::load_path(path.clone())?;
    assert!(!settings.sound_enabled);
    assert!(settings.save(Edit::Sound(true)).is_err());
    assert_eq!(fs::read_to_string(&path)?, original);
    assert_eq!(fs::read_dir(temp.path())?.count(), 1);
    fs::write(&path, " ".repeat(1024 * 1024 + 1))?;
    assert!(matches!(persistence::read(&path), Err(Error::TooLarge)));
    Ok(())
}

fn source(error: &crate::Error) -> Option<&Error> {
    let mut cause: &(dyn std::error::Error + 'static) = error;
    loop {
        if let Some(error) = cause.downcast_ref::<Error>() {
            return Some(error);
        }
        cause = cause.source()?;
    }
}

#[test]
fn defaults_and_path_precedence_without_environment_mutation() -> anyhow::Result<()> {
    let settings = parsed("")?;
    assert_eq!(settings.theme_name, "catppuccin");
    assert_eq!(settings.indicators, IndicatorStyle::Dots);
    assert!(settings.sound_enabled);
    assert_eq!(settings.toast_delivery, ToastDelivery::Off);
    assert_eq!(settings.toast_delay_seconds, 1);
    assert_eq!(settings.toast_position, ToastPosition::BottomRight);
    assert!(settings.clipboard.enabled);
    assert_eq!(settings.clipboard.position, ClipboardPosition::BottomCenter);
    assert_eq!(
        settings.name_prompts,
        NamePrompts {
            tab: true,
            workspace: false
        }
    );
    let temp = tempfile::tempdir()?;
    let missing = temp.path().join("missing/config.toml");
    assert_eq!(
        Settings::load_path(missing.clone())?.theme_name,
        "catppuccin"
    );
    assert!(!missing.parent().is_some_and(Path::exists));
    Ok(())
}

#[test]
fn exact_upstream_fields_and_legacy_toast_precedence() -> anyhow::Result<()> {
    let settings = parsed(
        r#"
[ui]
status_indicators = "symbols"
[ui.sound]
enabled = true
[ui.toast]
enabled = true
delivery = "system"
delay_seconds = 3600
[ui.toast.herdr]
position = "top-left"
[ui.toast.clipboard]
enabled = false
position = "top-center"
"#,
    )?;
    assert_eq!(settings.indicators, IndicatorStyle::Symbols);
    assert_eq!(settings.toast_delivery, ToastDelivery::System);
    assert_eq!(settings.toast_delay_seconds, 3600);
    assert_eq!(settings.toast_position, ToastPosition::TopLeft);
    assert_eq!(settings.clipboard.position, ClipboardPosition::TopCenter);
    assert!(!settings.clipboard.enabled);
    assert!(settings.sound_enabled);
    assert!(!parsed("[ui.sound]\nenabled = false")?.sound_enabled);
    assert_eq!(
        parsed("[ui.toast]\nenabled = true")?.toast_delivery,
        ToastDelivery::Herdr
    );
    assert_eq!(
        parsed("[ui.toast]\nenabled = false\ndelivery = 'terminal'")?.toast_delivery,
        ToastDelivery::Terminal
    );
    Ok(())
}

#[test]
fn sidebar_collapse_defaults_compact_expanded_and_parses_upstream_values() -> anyhow::Result<()> {
    let defaults = parsed("")?;
    assert_eq!(
        defaults.sidebar_collapsed_mode,
        SidebarCollapsedMode::Compact
    );
    assert!(!defaults.sidebar_start_collapsed);
    let set = parsed("[ui]\nsidebar_collapsed_mode = 'hidden'\nsidebar_start_collapsed = true\n")?;
    assert_eq!(set.sidebar_collapsed_mode, SidebarCollapsedMode::Hidden);
    assert!(set.sidebar_start_collapsed);
    assert_eq!(
        parsed("[ui]\nsidebar_collapsed_mode = 'compact'")?.sidebar_collapsed_mode,
        SidebarCollapsedMode::Compact
    );
    // A mode from a newer Herdr falls back alone; its neighbour still applies.
    let newer = parsed("[ui]\nsidebar_collapsed_mode = 'rail'\nsidebar_start_collapsed = true\n")?;
    assert_eq!(newer.sidebar_collapsed_mode, SidebarCollapsedMode::Compact);
    assert!(newer.sidebar_start_collapsed);
    Ok(())
}

#[test]
fn name_prompts_follow_both_ui_keys() -> anyhow::Result<()> {
    let flipped = parsed("[ui]\nprompt_new_tab_name = false\nprompt_new_workspace_name = true")?;
    assert_eq!(
        flipped.name_prompts,
        NamePrompts {
            tab: false,
            workspace: true
        }
    );
    // Each key keeps its own default when only the other is set.
    assert_eq!(
        parsed("[ui]\nprompt_new_workspace_name = true")?.name_prompts,
        NamePrompts {
            tab: true,
            workspace: true
        }
    );
    // A value this build cannot read keeps Herdr's default.
    assert_eq!(
        parsed("[ui]\nprompt_new_tab_name = 'no'")?.name_prompts,
        NamePrompts::default()
    );
    Ok(())
}

#[test]
fn values_from_a_newer_herdr_fall_back_one_by_one() -> anyhow::Result<()> {
    // Each value this build cannot read keeps its own default.
    for text in [
        "[ui]\nstatus_indicators = 'bars'",
        "[ui]\nagent_panel_sort = 'grouped'",
        "[ui.sound]\nenabled = 'true'",
        "[theme]\nauto_switch = 1",
        "[theme.custom]\nred = 123",
        "[theme.custom]\naccent = 123",
        "[ui.toast]\ndelay_seconds = -1",
        "[ui.toast]\ndelay_seconds = 3601",
        "[ui.toast]\ndelivery = 'pager'",
        "[ui.toast.herdr]\nposition = 'top-center'",
        "[ui]\nsidebar_collapsed_mode = 'rail'",
        "[ui]\nsidebar_start_collapsed = 'yes'",
        "[ui.toast.clipboard]\nposition = 'middle'\nenabled = 2",
        "theme = 'catppuccin'",
        "ui = 1",
    ] {
        let settings = parsed(text)?;
        let defaults = parsed("")?;
        assert_eq!(settings.indicators, defaults.indicators, "{text}");
        assert_eq!(settings.agent_sort, defaults.agent_sort, "{text}");
        assert_eq!(settings.sound_enabled, defaults.sound_enabled, "{text}");
        assert_eq!(settings.toast_delivery, defaults.toast_delivery, "{text}");
        assert_eq!(settings.toast_delay_seconds, 1, "{text}");
        assert_eq!(settings.toast_position, defaults.toast_position, "{text}");
        assert_eq!(
            settings.clipboard.enabled, defaults.clipboard.enabled,
            "{text}"
        );
        assert_eq!(
            settings.clipboard.position, defaults.clipboard.position,
            "{text}"
        );
        assert_eq!(settings.theme_name, defaults.theme_name, "{text}");
        assert_eq!(settings.palettes, defaults.palettes, "{text}");
        assert_eq!(
            settings.sidebar_collapsed_mode, defaults.sidebar_collapsed_mode,
            "{text}"
        );
        assert_eq!(
            settings.sidebar_start_collapsed, defaults.sidebar_start_collapsed,
            "{text}"
        );
    }
    // Readable neighbours of an unreadable value still apply.
    let settings = parsed(
        "[theme]\nname = 'nord'\nauto_switch = 'sometimes'\n\
         [ui]\nstatus_indicators = 'symbols'\nfuture = 1\n\
         [ui.toast]\nenabled = true\ndelivery = 'pager'\ndelay_seconds = 9\n\
         [ui.toast.herdr]\nposition = 'top-left'\n\
         [ui.toast.clipboard]\nenabled = false\nposition = 'middle'",
    )?;
    assert_eq!(settings.theme_name, "nord");
    assert_eq!(settings.indicators, IndicatorStyle::Symbols);
    assert_eq!(settings.toast_delivery, ToastDelivery::Herdr);
    assert_eq!(settings.toast_delay_seconds, 9);
    assert_eq!(settings.toast_position, ToastPosition::TopLeft);
    assert!(!settings.clipboard.enabled);
    assert_eq!(settings.clipboard.position, ClipboardPosition::BottomCenter);
    Ok(())
}

#[test]
fn malformed_toml_keeps_typed_sources() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    fs::write(&path, "[broken")?;
    let error = Settings::load_path(path)
        .err()
        .ok_or_else(|| anyhow::anyhow!("accepted malformed TOML"))?;
    assert!(matches!(source(&error), Some(Error::Parse(_))));
    use std::error::Error as _;
    assert!(
        source(&error)
            .and_then(|error| error.source())
            .is_some_and(|source| source.is::<toml::de::Error>())
    );
    Ok(())
}

#[test]
fn agent_panel_sort_reads_upstream_spellings() -> anyhow::Result<()> {
    use crate::preferences::AgentSort;
    for (text, expected) in [
        ("", AgentSort::Grouped),
        ("[ui]\nagent_panel_sort = 'spaces'", AgentSort::Grouped),
        ("[ui]\nagent_panel_sort = 'workspaces'", AgentSort::Grouped),
        ("[ui]\nagent_panel_sort = 'priority'", AgentSort::Priority),
    ] {
        assert_eq!(parsed(text)?.agent_sort, expected, "{text}");
    }
    Ok(())
}

#[test]
fn shared_sound_reader_only_validates_the_enabled_switch() -> anyhow::Result<()> {
    for text in [
        "",
        "[ui.sound]",
        "[ui.sound]\npath = 42\n[ui.sound.agents]\nclaude = true",
    ] {
        assert!(parsed(text)?.sound_enabled);
    }
    assert!(!parsed("[ui.sound]\nenabled = false\nagents = 'backend-owned'")?.sound_enabled);
    Ok(())
}

#[test]
fn tab_bar_and_copy_on_select_follow_herdr_defaults_and_values() -> anyhow::Result<()> {
    let defaults = parsed("")?;
    assert!(defaults.copy_on_select);
    assert_eq!(defaults.tab_bar_position, TabBarPosition::Top);
    assert!(!defaults.hide_tab_bar_when_single_tab);
    let set = parsed(
        "[ui]\ncopy_on_select = false\ntab_bar_position = 'bottom'\nhide_tab_bar_when_single_tab = true",
    )?;
    assert!(!set.copy_on_select);
    assert_eq!(set.tab_bar_position, TabBarPosition::Bottom);
    assert!(set.hide_tab_bar_when_single_tab);
    // Like Herdr, a value this build does not know keeps the default.
    let unknown = parsed(
        "[ui]\ncopy_on_select = 'sometimes'\ntab_bar_position = 'left'\nhide_tab_bar_when_single_tab = 1",
    )?;
    assert!(unknown.copy_on_select);
    assert_eq!(unknown.tab_bar_position, TabBarPosition::Top);
    assert!(!unknown.hide_tab_bar_when_single_tab);
    Ok(())
}

#[cfg(unix)]
#[test]
fn tab_bar_and_copy_on_select_edits_keep_comments() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let original = "[ui]\ncopy_on_select = true # copy\nfuture = 1\n";
    fs::write(&path, original)?;
    let settings = Settings::load_path(path.clone())?
        .save(Edit::CopyOnSelect(false))?
        .save(Edit::TabBarPosition(TabBarPosition::Bottom))?
        .save(Edit::HideSingleTabBar(true))?;
    assert!(!settings.copy_on_select);
    assert_eq!(settings.tab_bar_position, TabBarPosition::Bottom);
    assert!(settings.hide_tab_bar_when_single_tab);
    assert_eq!(
        fs::read_to_string(&path)?,
        "[ui]\ncopy_on_select = false # copy\nfuture = 1\ntab_bar_position = \"bottom\"\nhide_tab_bar_when_single_tab = true\n"
    );
    let settings = settings.save(Edit::TabBarPosition(TabBarPosition::Top))?;
    assert_eq!(settings.tab_bar_position, TabBarPosition::Top);
    Ok(())
}

#[cfg(unix)]
#[test]
fn sound_edits_preserve_paths_per_agent_policy_and_unknown_fields_verbatim() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let original = "[ui.sound] # audio\nenabled = true # switch\npath = 'all.mp3'\ndone_path = 'done.mp3' # done\nrequest_path = 'request.mp3'\nfuture = { key = 42 }\n[ui.sound.agents] # policy\ndroid = 'default'\nclaude = 'off'\nfuture-agent = 'new-policy'\n";
    fs::write(&path, original)?;
    let settings = Settings::load_path(path.clone())?.save(Edit::Sound(false))?;
    assert!(!settings.sound_enabled);
    assert_eq!(
        fs::read_to_string(&path)?,
        original.replace("enabled = true", "enabled = false")
    );
    assert!(settings.save(Edit::Sound(true))?.sound_enabled);
    assert_eq!(fs::read_to_string(&path)?, original);
    Ok(())
}

#[test]
#[cfg(unix)]
fn edits_preserve_comments_unknown_fields_and_disable_auto_switch() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let original = "# shared\nfuture = ['keep', 12]\n[theme] # theme\nname = 'nord' # selected\nauto_switch = true # follow\n[theme.custom]\nred = '#123456'\n[ui]\nstatus_indicators = 'dots' # indicators\n[ui.sound]\nenabled = true # audible\npath = 'keep.mp3'\n[ui.toast]\n# legacy comment\nenabled = true # legacy inline\ndelay_seconds = 2 # wait\n[unknown]\nvalue = 'untouched'\n";
    fs::write(&path, original)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
    let settings = Settings::load_path(path.clone())?.save(Edit::Theme("Dracula".into()))?;
    assert_eq!(
        fs::read_to_string(&path)?,
        original
            .replace("'nord'", "\"dracula\"")
            .replace("auto_switch = true", "auto_switch = false")
    );
    assert_eq!(settings.theme(false)?, settings.theme(true)?);
    assert_eq!(settings.status_color(AgentStatus::Blocked, false), 0x123456);
    let settings = settings
        .save(Edit::Indicators(IndicatorStyle::Symbols))?
        .save(Edit::Sound(false))?
        .save(Edit::Toasts(ToastDelivery::Terminal))?;
    assert_eq!(settings.indicators, IndicatorStyle::Symbols);
    assert!(!settings.sound_enabled);
    assert_eq!(settings.toast_delivery, ToastDelivery::Terminal);
    let text = fs::read_to_string(&path)?;
    for kept in [
        "# shared",
        "# selected",
        "# follow",
        "# indicators",
        "# audible",
        "# wait",
        "# legacy comment",
        "# legacy inline",
        "future = ['keep', 12]",
        "value = 'untouched'",
        "path = 'keep.mp3'",
    ] {
        assert!(text.contains(kept), "missing {kept}: {text}");
    }
    let document: toml::Value = toml::from_str(&text)?;
    assert!(document["ui"]["toast"].get("enabled").is_none());
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o640);
    assert!(fs::read_dir(temp.path())?.all(|entry| {
        entry.is_ok_and(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp"))
    }));
    Ok(())
}

#[test]
#[cfg(unix)]
fn inline_tables_and_dotted_keys_remain_valid() -> anyhow::Result<()> {
    for text in [
        "ui = { sound = { enabled = true }, toast = { enabled = true, future = 42 } }\n",
        "ui.sound.enabled = true\nui.toast.enabled = true\n",
    ] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("config.toml");
        fs::write(&path, text)?;
        let settings = Settings::load_path(path.clone())?
            .save(Edit::Sound(false))?
            .save(Edit::Toasts(ToastDelivery::Herdr))?;
        assert!(!settings.sound_enabled);
        assert_eq!(settings.toast_delivery, ToastDelivery::Herdr);
        Settings::load_path(path)?;
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn saves_reject_changed_deleted_created_or_replaced_originals() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let settings = Settings::load_path(path.clone())?;
    fs::write(&path, "# concurrently created\n")?;
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("clobbered creation"))?;
    assert!(matches!(source(&error), Some(Error::Conflict)));
    let settings = Settings::load_path(path.clone())?;
    fs::write(&path, "# unrelated edit\n")?;
    assert!(matches!(
        source(
            &settings
                .save(Edit::Sound(false))
                .err()
                .ok_or_else(|| anyhow::anyhow!("clobbered edit"))?
        ),
        Some(Error::Conflict)
    ));
    assert_eq!(fs::read_to_string(&path)?, "# unrelated edit\n");
    let settings = Settings::load_path(path.clone())?;
    fs::remove_file(&path)?;
    assert!(settings.save(Edit::Sound(false)).is_err());
    let settings = Settings::load_path(path.clone())?.save(Edit::Sound(false))?;
    let stale = settings.clone();
    settings.save(Edit::Sound(true))?;
    assert!(
        stale
            .save(Edit::Indicators(IndicatorStyle::Symbols))
            .is_err()
    );
    let settings = Settings::load_path(path.clone())?;
    let replacement = temp.path().join("replacement");
    fs::write(&replacement, fs::read_to_string(&path)?)?;
    fs::rename(replacement, &path)?;
    assert!(settings.save(Edit::Sound(false)).is_err());
    Ok(())
}

#[test]
#[cfg(unix)]
fn symlinks_hardlinks_permissions_and_size_are_protected() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let target = temp.path().join("target");
    fs::write(&target, "# untouched\n")?;
    symlink(&target, &path)?;
    assert!(matches!(persistence::read(&path), Err(Error::UnsafePath)));
    fs::remove_file(&path)?;
    let settings = Settings::load_path(path.clone())?;
    symlink(&target, &path)?;
    assert!(settings.save(Edit::Sound(false)).is_err());
    assert_eq!(fs::read_to_string(&target)?, "# untouched\n");
    fs::remove_file(&path)?;
    fs::hard_link(&target, &path)?;
    assert!(matches!(persistence::read(&path), Err(Error::UnsafePath)));
    fs::remove_file(&path)?;
    fs::write(&path, "")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666))?;
    assert!(matches!(persistence::read(&path), Err(Error::UnsafePath)));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    fs::write(&path, " ".repeat(1024 * 1024 + 1))?;
    assert!(matches!(persistence::read(&path), Err(Error::TooLarge)));
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o777))?;
    assert!(matches!(persistence::read(&path), Err(Error::UnsafePath)));
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[test]
fn every_palette_status_overrides_and_auto_switch() -> anyhow::Result<()> {
    assert_eq!(THEME_NAMES.len(), 18);
    for name in THEME_NAMES {
        let settings = parsed(&format!("[theme]\nname = '{name}'"))?;
        let theme = settings.theme(false)?;
        assert_eq!(
            theme.palette[255],
            crate::config::Theme::default().palette[255]
        );
        assert_eq!(theme.cursor, theme.foreground);
        for status in [
            AgentStatus::Working,
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Idle,
            AgentStatus::Unknown,
        ] {
            assert!(settings.status_color(status, false) <= 0xffffff);
        }
    }
    let settings = parsed("")?;
    assert_eq!(settings.theme(false)?.surface, 0x181825);
    assert_eq!(settings.theme(false)?.foreground, 0xcdd6f4);
    assert_eq!(settings.status_color(AgentStatus::Working, false), 0xf9e2af);
    assert_eq!(settings.status_color(AgentStatus::Blocked, false), 0xf38ba8);
    assert_eq!(settings.status_color(AgentStatus::Done, false), 0x94e2d5);
    assert_eq!(settings.status_color(AgentStatus::Idle, false), 0xa6e3a1);
    assert_eq!(settings.status_color(AgentStatus::Unknown, false), 0x6c7086);
    let settings = parsed(
        "[theme]\nname = 'nord'\nauto_switch = true\ndark_name = 'gruvbox'\nlight_name = 'latte'\n[theme.custom]\nred = '#123'\naccent = 'rgb(1, 2, 3)'\n[theme.custom.light]\nred = '#abcdef'\nsidebar_bg = '#fefefe'",
    )?;
    assert_eq!(settings.theme(false)?.surface, 0x282828);
    assert_eq!(settings.theme(true)?.surface, 0xeff1f5);
    // `sidebar_bg` colors the sidebar alone, as in Herdr, never the window.
    assert_eq!(settings.theme(true)?.background, 0xe6e9ef);
    assert_eq!(settings.theme(true)?.sidebar, Some(0xfefefe));
    assert_eq!(settings.theme(true)?.sidebar_background(), 0xfefefe);
    assert_eq!(settings.theme(false)?.sidebar, None);
    assert_eq!(settings.theme(false)?.sidebar_background(), 0x282828);
    assert_eq!(settings.theme(false)?.palette[5], 0x010203);
    assert_eq!(settings.status_color(AgentStatus::Blocked, false), 0x112233);
    assert_eq!(settings.status_color(AgentStatus::Blocked, true), 0xabcdef);
    assert_eq!(
        parsed("[theme]\nname = 'one-dark'\nauto_switch = true")?
            .theme(true)?
            .surface,
        0xfafafa
    );
    assert_eq!(
        parsed("[theme]\nname = 'unknown'")?.theme(false)?,
        parsed("")?.theme(false)?
    );
    // Invalid Unicode hex must never slice a code point or panic.
    assert_eq!(
        parsed("[theme.custom]\nred = '#\u{e9}\u{e9}\u{e9}'")?
            .status_color(AgentStatus::Blocked, false),
        0x008080
    );
    Ok(())
}

#[test]
fn invalid_theme_edits_do_not_create_config_or_parent() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("missing/config.toml");
    let settings = Settings::load_path(path.clone())?;
    let error = settings
        .save(Edit::Theme("not-a-theme".into()))
        .err()
        .ok_or_else(|| anyhow::anyhow!("saved unknown theme"))?;
    assert!(matches!(source(&error), Some(Error::Theme(_))));
    assert!(!path.parent().is_some_and(Path::exists));
    #[cfg(unix)]
    {
        let settings = settings.save(Edit::Sound(false))?;
        assert!(!settings.sound_enabled);
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn advisory_lock_is_nonblocking_and_released_after_saves() -> anyhow::Result<()> {
    use rustix::fs::{FlockOperation, flock};
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let settings = Settings::load_path(path)?.save(Edit::Sound(false))?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp.path().join(".config.toml.gpui-lock"))?;
    flock(&lock, FlockOperation::NonBlockingLockExclusive)?;
    let error = settings
        .save(Edit::Sound(true))
        .err()
        .ok_or_else(|| anyhow::anyhow!("ignored held lock"))?;
    assert!(matches!(source(&error), Some(Error::Busy)));
    flock(&lock, FlockOperation::Unlock)?;
    drop(lock);
    let settings = settings.save(Edit::Sound(true))?;
    assert!(settings.sound_enabled);
    assert!(!settings.save(Edit::Sound(false))?.sound_enabled);
    Ok(())
}

#[test]
#[cfg(unix)]
fn parent_replacement_and_permission_changes_are_conflicts() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let parent = temp.path().join("config");
    fs::create_dir(&parent)?;
    let path = parent.join("config.toml");
    let settings = Settings::load_path(path.clone())?.save(Edit::Sound(true))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("ignored mode change"))?;
    assert!(matches!(source(&error), Some(Error::Conflict)));
    let settings = Settings::load_path(path.clone())?;
    let moved = temp.path().join("moved");
    fs::rename(&parent, &moved)?;
    fs::create_dir(&parent)?;
    fs::write(&path, "[ui.sound]\nenabled = true\n")?;
    assert!(settings.save(Edit::Sound(false)).is_err());
    fs::remove_dir_all(&parent)?;
    symlink(&moved, &parent)?;
    assert!(settings.save(Edit::Sound(false)).is_err());
    assert!(Settings::load_path(path).is_err());
    assert!(Settings::load_path(moved.join("config.toml"))?.sound_enabled);
    Ok(())
}

#[test]
fn upstream_aliases_fallbacks_and_legacy_override_precedence() -> anyhow::Result<()> {
    for (alias, canonical) in [
        ("Catppuccin Mocha", "catppuccin"),
        ("light", "catppuccin-latte"),
        ("tokyonight", "tokyo-night"),
        ("tokyo_day", "tokyo-night-day"),
        ("gruvbox-dark", "gruvbox"),
        ("onedark", "one-dark"),
        ("onelight", "one-light"),
        ("solarized-dark", "solarized"),
        ("lotus", "kanagawa-lotus"),
        ("rosepine", "rose-pine"),
        ("dawn", "rose-pine-dawn"),
    ] {
        assert_eq!(palette::canonical(alias), Some(canonical));
        assert_eq!(
            parsed(&format!("[theme]\nname = '{alias}'"))?.palettes,
            parsed(&format!("[theme]\nname = '{canonical}'"))?.palettes,
        );
    }
    // Upstream normalizes separators and case, but does not trim names.
    assert_eq!(palette::canonical(" nord "), None);
    let unknown = parsed("[theme]\nname = 'unknown'\nauto_switch = true")?;
    assert_eq!(unknown.theme(false)?, parsed("")?.theme(false)?);
    assert_eq!(
        unknown.theme(true)?,
        parsed("[theme]\nname = 'latte'")?.theme(false)?
    );
    let legacy = parsed(
        "[ui]\naccent = '#123456'\n[theme]\nauto_switch = true\n[theme.custom.light]\naccent = '#abcdef'",
    )?;
    assert_eq!(legacy.theme(false)?.palette[5], 0x123456);
    assert_eq!(legacy.theme(true)?.palette[5], 0xabcdef);
    let custom = parsed(
        "[ui]\naccent = '#123456'\n[theme.custom]\naccent = '#abcdef'\n[theme.custom.light]\naccent = '#ffffff'",
    )?;
    assert_eq!(custom.theme(false)?.palette[5], 0xabcdef);
    assert_eq!(custom.theme(true)?, custom.theme(false)?);
    Ok(())
}

#[test]
fn prepared_indicator_palettes_and_parse_limit() -> anyhow::Result<()> {
    let settings = parsed(&format!(
        "[theme.custom]\nred = '{}#123456'",
        " ".repeat(100_000)
    ))?;
    let prepared = settings.colors(false);
    for _ in 0..1000 {
        assert!(std::ptr::eq(prepared, settings.colors(false)));
        assert_eq!(settings.status_color(AgentStatus::Blocked, false), 0x123456);
    }
    assert!(matches!(
        parsed(&" ".repeat(1024 * 1024 + 1)),
        Err(Error::TooLarge)
    ));
    Ok(())
}

#[test]
#[cfg(unix)]
fn symlink_ancestors_cannot_redirect_directory_creation() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let target = temp.path().join("target");
    fs::create_dir(&target)?;
    let alias = temp.path().join("alias");
    let path = alias.join("new/config.toml");
    let settings = Settings::load_path(path.clone())?;
    symlink(&target, &alias)?;
    assert!(matches!(persistence::read(&path), Err(Error::UnsafePath)));
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("followed symlink ancestor"))?;
    assert!(matches!(source(&error), Some(Error::UnsafePath)));
    assert!(!target.join("new").exists());
    Ok(())
}

#[test]
#[cfg(unix)]
fn in_place_file_revision_changes_are_not_overwritten() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    fs::write(&path, "# unchanged contents\n")?;
    let settings = Settings::load_path(path.clone())?;
    // Explicit timestamps avoid relying on filesystem clock resolution or sleeps.
    fs::File::options().write(true).open(&path)?.set_times(
        fs::FileTimes::new()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(123)),
    )?;
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("ignored changed revision"))?;
    assert!(matches!(source(&error), Some(Error::Conflict)));
    assert_eq!(fs::read_to_string(&path)?, "# unchanged contents\n");
    Ok(())
}

#[test]
fn diagnostics_do_not_dump_unrelated_shared_config() -> anyhow::Result<()> {
    let settings =
        parsed("private_value = 'do-not-log-me'\n[ui.sound]\npath = 'private-sound-path'")?;
    let diagnostic = format!("{settings:?}");
    assert!(!diagnostic.contains("do-not-log-me"));
    assert!(!diagnostic.contains("private-sound-path"));
    assert!(diagnostic.contains("clipboard"));
    #[cfg(unix)]
    {
        let error = Error::Committed(std::io::Error::other("sync failed"));
        assert!(
            std::error::Error::source(&error).is_some_and(|cause| cause.is::<std::io::Error>())
        );
        assert!(error.to_string().contains("reload"));
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn lock_symlinks_and_nonregular_configs_are_rejected() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let settings = Settings::load_path(path.clone())?;
    let target = temp.path().join("target");
    fs::write(&target, "unchanged")?;
    symlink(&target, temp.path().join(".config.toml.gpui-lock"))?;
    let error = settings
        .save(Edit::Sound(false))
        .err()
        .ok_or_else(|| anyhow::anyhow!("followed lock symlink"))?;
    assert!(matches!(source(&error), Some(Error::UnsafePath)));
    assert_eq!(fs::read_to_string(&target)?, "unchanged");
    assert!(!path.exists());
    fs::create_dir(&path)?;
    assert!(matches!(persistence::read(&path), Err(Error::UnsafePath)));
    Ok(())
}

/// Herdr paints `sidebar_bg` on the sidebar and nowhere else, whether set for
/// both modes or per mode; built-in themes leave it unset.
#[test]
fn sidebar_background_colors_only_the_sidebar() -> anyhow::Result<()> {
    let plain = parsed("[theme]\nname = 'catppuccin'")?.theme(false)?;
    assert_eq!(plain.sidebar, None);
    assert_eq!(plain.sidebar_background(), plain.surface);
    for light in [false, true] {
        let base = parsed("[theme]\nname = 'catppuccin'\nauto_switch = true")?.theme(light)?;
        let common = parsed(
            "[theme]\nname = 'catppuccin'\nauto_switch = true\n[theme.custom]\nsidebar_bg = '#0d0e0f'",
        )?
        .theme(light)?;
        assert_eq!(common.sidebar, Some(0x0d0e0f), "light {light}");
        assert_eq!(
            crate::config::Theme {
                sidebar: None,
                ..common
            },
            base,
            "light {light}: nothing but the sidebar changes"
        );
        let per_mode = parsed(
            "[theme]\nname = 'catppuccin'\nauto_switch = true\n[theme.custom]\nsidebar_bg = '#0d0e0f'\n[theme.custom.light]\nsidebar_bg = '#f0f1f2'\n[theme.custom.dark]\nsidebar_bg = '#101112'",
        )?
        .theme(light)?;
        assert_eq!(
            per_mode.sidebar,
            Some(if light { 0xf0f1f2 } else { 0x101112 })
        );
    }
    Ok(())
}

/// `mouse_scroll_lines` follows Herdr: three by default, and a count Herdr
/// would refuse keeps the default instead of failing the file.
#[test]
fn mouse_scroll_lines_default_parse_and_fall_back() -> anyhow::Result<()> {
    assert_eq!(parsed("")?.mouse_scroll_lines.get(), 3);
    assert_eq!(DEFAULT_MOUSE_SCROLL_LINES.get(), 3);
    assert_eq!(
        parsed("[ui]\nmouse_scroll_lines = 7")?
            .mouse_scroll_lines
            .get(),
        7
    );
    for invalid in ["0", "-2", "70000", "'five'", "1.5"] {
        let settings = parsed(&format!(
            "[ui]\nmouse_scroll_lines = {invalid}\ncopy_on_select = false"
        ))?;
        assert_eq!(settings.mouse_scroll_lines.get(), 3, "value {invalid}");
        assert!(!settings.copy_on_select, "value {invalid} kept the rest");
    }
    Ok(())
}

/// `right_click_passthrough_modifier` accepts Herdr's grammar and matches
/// only the exact combination; what Herdr rejects, or GPUI cannot report,
/// leaves right-clicks with the menu.
#[test]
fn right_click_passthrough_modifier_parses_herdr_grammar() -> anyhow::Result<()> {
    use gpui::Modifiers;
    let modifier = |value: &str| -> anyhow::Result<RightClickPassthrough> {
        Ok(
            parsed(&format!("[ui]\nright_click_passthrough_modifier = {value}"))?
                .right_click_passthrough,
        )
    };
    assert_eq!(
        parsed("")?.right_click_passthrough,
        RightClickPassthrough::default()
    );
    let control = Modifiers {
        control: true,
        ..Default::default()
    };
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let platform = Modifiers {
        platform: true,
        ..Default::default()
    };
    for (value, expected) in [
        ("'ctrl'", control),
        ("'Control'", control),
        ("'alt'", alt),
        ("'option'", alt),
        ("'cmd'", platform),
        ("'command'", platform),
        ("'super'", platform),
        (
            "' ctrl + alt '",
            Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
        ),
    ] {
        let passthrough = modifier(value)?;
        assert!(passthrough.matches(expected), "value {value}");
        assert!(!passthrough.matches(Modifiers::default()), "value {value}");
        assert!(
            !passthrough.matches(Modifiers {
                shift: true,
                ..expected
            }),
            "value {value} must match exactly"
        );
    }
    for disabled in [
        "''",
        "'off'",
        "'None'",
        "'disabled'",
        "'meta'",
        "'ctrl+hyper'",
        "'shift'",
        "'ctrl+shift'",
        "'ctrl+'",
        "'banana'",
        "3",
    ] {
        let passthrough = modifier(disabled)?;
        assert_eq!(
            passthrough,
            RightClickPassthrough::default(),
            "value {disabled}"
        );
        for held in [Modifiers::default(), control, alt, platform] {
            assert!(!passthrough.matches(held), "value {disabled}");
        }
    }
    Ok(())
}

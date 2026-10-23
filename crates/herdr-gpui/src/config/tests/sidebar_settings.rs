use super::*;

/// The daemon's `state_text` token turns the GUI's status word on for the
/// agents whose rows name it: an agent's `rows_by_agent` entry replaces
/// `rows` for that agent only. Rows without it, or a file the GUI cannot
/// use, leave it off.
#[test]
fn daemon_sidebar_state_text_turns_agent_status_words_on() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let daemon = temp.0.join("config.toml");
    // Expected for Claude, Codex, and an agent the daemon did not identify.
    for (text, expected) in [
        ("", [false; 3]),
        ("[ui]\nstatus_indicators = \"dots\"\n", [false; 3]),
        (
            "[ui.sidebar.agents]\nrows = [[\"state_icon\", \"workspace\", \"tab\"], [\"agent\"]]\n",
            [false; 3],
        ),
        (
            "[ui.sidebar.agents]\nrows = [[\"state_icon\", \"agent\", \"state_text\"], [\"agent\"]]\n",
            [true; 3],
        ),
        (
            "[ui.sidebar.agents]\nrows = [[{ token = \"state_text\", dim = true }]]\n",
            [true; 3],
        ),
        (
            "[ui.sidebar.agents.rows_by_agent]\nclaude = [[\"state_icon\", \"state_text\"]]\n",
            [true, false, false],
        ),
        (
            "[ui.sidebar.agents.rows_by_agent]\nclaude = [[\"agent\"]]\n",
            [false; 3],
        ),
        (
            "[ui.sidebar.agents]\nrows = [[\"state_text\"]]\n\
                 [ui.sidebar.agents.rows_by_agent]\nclaude = [[\"agent\"]]\n",
            [false, true, true],
        ),
        (
            "[ui.sidebar.agents]\nrows = [[\"state_text\", \"bogus\"]]",
            [false; 3],
        ),
        ("not toml", [false; 3]),
    ] {
        fs::write(&daemon, text)?;
        let settings = daemon_settings(&daemon).sidebar_layout.agents;
        assert_eq!(
            [
                settings.shows_status_text(Some("claude")),
                settings.shows_status_text(Some("codex")),
                settings.shows_status_text(None),
            ],
            expected,
            "{text}"
        );
    }
    let off = AgentLayout::default();
    assert_eq!(
        daemon_settings(&temp.0).sidebar_layout.agents,
        off,
        "a directory is not a config"
    );
    assert_eq!(
        daemon_settings(&temp.0.join("absent.toml"))
            .sidebar_layout
            .agents,
        off
    );

    // Oversized files are skipped rather than parsed on every config load.
    let mut oversized = "[ui.sidebar.agents]\nrows = [[\"state_text\"]]\n".to_owned();
    oversized.push_str(&"# pad\n".repeat(MAX_DAEMON_CONFIG_BYTES as usize / 6));
    assert!(oversized.len() as u64 > MAX_DAEMON_CONFIG_BYTES);
    fs::write(&daemon, &oversized)?;
    assert_eq!(daemon_settings(&daemon).sidebar_layout.agents, off);
    Ok(())
}

#[test]
fn sidebar_layout_comes_from_the_daemon_config() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let daemon = temp.0.join("config.toml");
    let absent = daemon_settings(&temp.0.join("absent.toml"));
    assert_eq!(absent.sidebar_layout, SidebarLayout::default());

    fs::write(
        &daemon,
        "[ui.sidebar.agents]\nrows = [[\"agent\", \"$usage_ctx_ok\"]]\nrow_gap = 1\n[unrelated]\nx = 1\n[ui.toast.clipboard]\nenabled = false\n",
    )?;
    let settings = daemon_settings(&daemon);
    assert_eq!(settings.sidebar_layout.agents.rows.len(), 1);
    assert_eq!(settings.sidebar_layout.agents.row_gap, 1);
    assert_eq!(settings.sidebar_layout.spaces, SpaceLayout::default());
    assert!(!settings.clipboard_toast.enabled);
    let config = Config::parse_layers(["\n"], &settings)?;
    assert_eq!(config.sidebar_layout, settings.sidebar_layout);
    assert_eq!(
        Config::parse_layers(["[usage]\ninline = false"], &settings)?.sidebar_layout,
        settings.sidebar_layout
    );

    fs::write(
        &daemon,
        "[ui.toast.clipboard]\nenabled = false\n[ui.sidebar.agents]\nrows = [[\"bogus\"]]\n",
    )?;
    let settings = daemon_settings(&daemon);
    assert_eq!(settings.sidebar_layout, SidebarLayout::default());
    assert!(!settings.clipboard_toast.enabled);
    fs::write(&daemon, "not toml [")?;
    let settings = daemon_settings(&daemon);
    assert_eq!(settings.sidebar_layout, SidebarLayout::default());
    assert_eq!(settings.clipboard_toast, ClipboardToast::default());

    Ok(())
}

#[test]
fn every_layout_has_its_own_name_and_label() -> anyhow::Result<()> {
    assert_eq!(LayoutMode::NAMES, LayoutMode::ALL.map(LayoutMode::name));
    let labels: std::collections::HashSet<_> =
        LayoutMode::ALL.iter().map(|mode| mode.label()).collect();
    assert_eq!(labels.len(), LayoutMode::ALL.len());
    for mode in LayoutMode::ALL {
        let name = mode.name();
        assert_eq!(LayoutMode::try_from(name)?, mode);
        assert_eq!(
            Config::parse(&format!("layout = '{name}'"))?.layout.mode,
            mode
        );
        let table = Config::parse(&format!("[layout]\nmode = '{name}'\nsidebar_gap = 4"))?;
        assert_eq!(
            (table.layout.mode, table.layout.legacy_sidebar_gap),
            (mode, Some(4.))
        );
    }
    // Layouts with a design of their own fix their spacing.
    assert_eq!(
        (LayoutMode::Orca.density(), LayoutMode::Orca.style()),
        (Density::Comfortable, Style::Rounded)
    );
    // A second setting for rows no longer exists: ignored, not applied.
    let config = Config::parse("[layout]\nmode = 'minimal'\nrows = 'orca'")?;
    assert_eq!(config.layout.mode, LayoutMode::Minimal);
    assert_eq!(config.unknown_keys, ["layout.rows"]);
    assert!(Config::parse("layout = 'herdr'").is_err());
    Ok(())
}

#[test]
fn saving_a_layout_keeps_every_other_setting() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.local.toml");
    let mode = |path: &Path| -> anyhow::Result<Layout> {
        Ok(Config::parse(&fs::read_to_string(path)?)?.layout)
    };
    // A new install's plain name is replaced in place, comments and all,
    // and still layers over the managed file.
    Config::save_layout_path(LayoutMode::Orca, &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("layout = \"orca\""), "{text}");
    assert!(text.contains("# New installs start"), "{text}");
    let merged = Config::parse_layers([DEFAULT_CONFIG, text.as_str()], &Daemon::default())?;
    assert_eq!(merged.layout.mode, LayoutMode::Orca);
    // A table gets its mode beside the gap, and keeps its comments.
    fs::write(
        &path,
        "# mine\ntheme = 'Nord'\n\n[layout] # sidebar\nmode = 'compact'\nsidebar_gap = 4\n",
    )?;
    for chosen in LayoutMode::ALL {
        Config::save_layout_path(chosen, &path)?;
        let layout = mode(&path)?;
        assert_eq!((layout.mode, layout.legacy_sidebar_gap), (chosen, Some(4.)));
    }
    let text = fs::read_to_string(&path)?;
    assert!(
        text.contains("# mine") && text.contains("# sidebar"),
        "{text}"
    );
    assert_eq!(Config::parse(&text)?.theme, "Nord");
    // Inline tables and files without a layout work too.
    for original in ["layout = { sidebar_gap = 4 }\n", "theme = 'Nord'\n"] {
        fs::write(&path, original)?;
        Config::save_layout_path(LayoutMode::Minimal, &path)?;
        assert_eq!(mode(&path)?.mode, LayoutMode::Minimal, "{original}");
    }
    Ok(())
}

#[test]
fn compact_layout_is_opt_in() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
        Config::parse("[layout]")?,
        Config::parse("layout = 'normal'")?,
    ] {
        assert_eq!(config.layout.mode, LayoutMode::default());
        assert_eq!(config.layout.mode, LayoutMode::from(Density::Normal));
    }
    let config = Config::parse("layout = 'compact'")?;
    assert_eq!(config.layout.mode, LayoutMode::from(Density::Compact));
    assert_eq!(config.terminal_padding, Padding::default());
    assert_eq!(config.sidebar.size, Config::default().sidebar.size);
    let custom = Config::parse("[layout]\nmode = 'compact'\nsidebar_gap = 4")?;
    assert_eq!(custom.layout.mode, LayoutMode::from(Density::Compact));
    assert_eq!(custom.terminal_padding.x.start(), 4.);
    for density in [Density::Compact, Density::Normal, Density::Comfortable] {
        for style in [Style::Flat, Style::Rounded] {
            let mode = LayoutMode::new(density, style);
            let name = mode.to_string();
            assert_eq!(LayoutMode::try_from(name.as_str())?, mode);
            assert_eq!(
                Config::parse(&format!("layout = '{name}'"))?.layout.mode,
                mode
            );
            let config = Config::parse(&format!("[layout]\nmode = '{name}'\nsidebar_gap = 4"))?;
            assert_eq!(config.layout.mode, mode);
            assert_eq!(config.terminal_padding.x.start(), 4.);
        }
    }
    assert_eq!(
        LayoutMode::try_from("compact-rounded")?,
        LayoutMode::new(Density::Compact, Style::Rounded)
    );
    for name in [
        "rounded",
        "-rounded",
        "normal-",
        "Normal",
        "normal-rounded-rounded",
    ] {
        assert!(matches!(
            LayoutMode::try_from(name),
            Err(Error::UnknownLayout(unknown)) if unknown == name
        ));
    }
    for value in ["'unknown'", "'rounded'", "'normal-square'", "true", "1"] {
        assert!(matches!(
            Config::parse(&format!("layout = {value}")),
            Err(crate::Error::Toml(_))
        ));
    }
    Ok(())
}

#[test]
fn the_deprecated_sidebar_gap_defaults_to_flush_and_accepts_its_band() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        assert_eq!(config.layout, Layout::default());
        assert_eq!(config.terminal_padding.x.start(), 0.);
    }
    // An empty table keeps the default; only a written value replaces it.
    assert_eq!(Config::parse("[layout]")?.terminal_padding.x.start(), 0.);
    for (text, gap) in [
        ("[layout]\nsidebar_gap = 0", 0.),
        ("[layout]\nsidebar_gap = 12", 12.),
        ("[layout]\nsidebar_gap = 7.5", 7.5),
        ("[layout]\nsidebar_gap = 64", 64.),
    ] {
        let config = Config::parse(text)?;
        assert_eq!(config.terminal_padding.x.start(), gap);
        // Spacing alone leaves every other setting at its default.
        assert_eq!(config.theme, Config::default().theme);
        assert_eq!(config.terminal.size, Config::default().terminal.size);
    }
    assert!(matches!(
        Config::parse("[layout]\nsidebar_gap = 64.1"),
        Err(crate::Error::Settings(Error::InvalidSidebarGap))
    ));
    assert!(matches!(
        Config::parse("[layout]\nsidebar_gap = nan"),
        Err(crate::Error::Settings(Error::InvalidSidebarGap))
    ));
    Ok(())
}

#[test]
fn only_new_installs_start_with_the_rounded_comfortable_layout() -> anyhow::Result<()> {
    let rounded = LayoutMode::new(Density::Comfortable, Style::Rounded);
    let daemon = Path::new("absent.toml");
    // The managed defaults keep the flat layout for everyone else.
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.layout.mode,
        LayoutMode::default()
    );
    assert_eq!(Config::parse(LOCAL_CONFIG)?.layout.mode, rounded);

    let fresh = TempDirectory::new()?;
    let path = fresh.0.join("config-gpui.toml");
    assert_eq!(
        Config::load_startup_path(&path, daemon)?.layout.mode,
        rounded
    );
    assert_eq!(Config::load_path(&path, daemon)?.layout.mode, rounded);
    assert_eq!(
        fs::read_to_string(path.with_extension("local.toml"))?,
        LOCAL_CONFIG
    );
    // A later launch reads the seeded file, not the first-launch fallback.
    assert_eq!(
        Config::load_startup_path(&path, daemon)?.layout.mode,
        rounded
    );

    // Existing overrides without a layout keep the managed default.
    let existing = TempDirectory::new()?;
    let path = existing.0.join("config-gpui.toml");
    fs::write(path.with_extension("local.toml"), "theme = 'Nord'\n")?;
    for config in [
        Config::load_startup_path(&path, daemon)?,
        Config::load_path(&path, daemon)?,
    ] {
        assert_eq!(config.layout.mode, LayoutMode::default());
    }
    assert_eq!(
        fs::read_to_string(path.with_extension("local.toml"))?,
        "theme = 'Nord'\n"
    );

    // So does a personal config migrated from before local overrides.
    let legacy = TempDirectory::new()?;
    let path = legacy.0.join("config-gpui.toml");
    fs::write(&path, "theme = 'Nord'\n")?;
    for config in [
        Config::load_startup_path(&path, daemon)?,
        Config::load_path(&path, daemon)?,
    ] {
        assert_eq!(config.layout.mode, LayoutMode::default());
    }
    Ok(())
}

mod preference_writes {
    use super::*;
    use crate::config::preferences::Preference;
    use crate::config::{DeviceFooter, SelectMode};
    use std::fs;

    fn write(path: &Path, edit: Preference) -> Result<()> {
        Config::save_preference_path(edit, path)
    }

    #[test]
    fn sidebar_keys_round_trip_reset_and_keep_decor() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("local.toml");
        fs::write(
            &path,
            "[sidebar]\nfamily = 'Inter' # font\nindent = 8 # old\n",
        )?;
        write(&path, Preference::SidebarIndent(Some(14.)))?;
        write(&path, Preference::SidebarRowPadding(Some(2.)))?;
        write(&path, Preference::SidebarRowGap(Some(6.)))?;
        write(&path, Preference::SidebarHostGap(Some(10.)))?;
        write(&path, Preference::SidebarSelect(SelectMode::GroupDim))?;
        write(&path, Preference::SidebarDeviceFooter(DeviceFooter::Auto))?;
        write(
            &path,
            Preference::HostColor {
                host: "Personal".into(),
                color: Some(0x3a4a6b),
            },
        )?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("family = 'Inter' # font"), "{text}");
        assert!(
            text.contains("indent = 14 # old") || text.contains("indent = 14.0 # old"),
            "{text}"
        );
        let config = Config::parse(&text)?;
        assert_eq!(config.sidebar_style.overrides.indent, Some(14.));
        assert_eq!(config.sidebar_style.overrides.row_padding, Some(2.));
        assert_eq!(config.sidebar_style.overrides.gap, Some(6.));
        assert_eq!(config.sidebar_style.overrides.host_gap, Some(10.));
        assert_eq!(config.sidebar_style.select, SelectMode::GroupDim);
        assert_eq!(config.sidebar_style.device_footer, DeviceFooter::Auto);
        assert_eq!(config.sidebar_style.hosts.get("Personal"), Some(&0x3a4a6b));
        assert!(text.contains("Personal = \"#3a4a6b\""), "{text}");

        write(&path, Preference::SidebarIndent(None))?;
        write(
            &path,
            Preference::HostColor {
                host: "Personal".into(),
                color: None,
            },
        )?;
        let config = Config::parse(&fs::read_to_string(&path)?)?;
        assert_eq!(config.sidebar_style.overrides.indent, None);
        assert!(config.sidebar_style.hosts.is_empty());
        Ok(())
    }

    #[test]
    fn sidebar_keys_are_validated_atomically() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("local.toml");
        let original = "[sidebar]\nindent = 4\n";
        fs::write(&path, original)?;
        for (edit, key, max) in [
            (Preference::SidebarIndent(Some(49.)), "indent", 48.),
            (Preference::SidebarRowPadding(Some(-1.)), "row_padding", 16.),
            (Preference::SidebarRowPadding(Some(17.)), "row_padding", 16.),
            (Preference::SidebarRowGap(Some(33.)), "gap", 32.),
            (Preference::SidebarRowGap(Some(f32::NAN)), "gap", 32.),
            (Preference::SidebarHostGap(Some(49.)), "host_gap", 48.),
            (
                Preference::SidebarHostGap(Some(f32::INFINITY)),
                "host_gap",
                48.,
            ),
        ] {
            let Err(error) = write(&path, edit) else {
                anyhow::bail!("out-of-band value was accepted");
            };
            assert!(std::error::Error::source(&error).is_some());
            let crate::Error::Path { source: cause, .. } = &error else {
                anyhow::bail!("not path-wrapped: {error:?}");
            };
            let cause = Some(&**cause);
            assert!(
                matches!(cause, Some(crate::Error::Settings(Error::InvalidSidebarMetric { key: k, max: m })) if *k == key && *m == max),
                "{cause:?}"
            );
            assert_eq!(fs::read_to_string(&path)?, original);
        }
        Ok(())
    }

    #[test]
    fn host_colour_above_rrggbb_is_rejected_before_writing() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("local.toml");
        let original = "theme = 'Default'\n";
        fs::write(&path, original)?;
        let Err(error) = write(
            &path,
            Preference::HostColor {
                host: "Work".into(),
                color: Some(0x100_0000),
            },
        ) else {
            anyhow::bail!("oversized colour was accepted");
        };
        assert!(std::error::Error::source(&error).is_some());
        let crate::Error::Path { source: cause, .. } = &error else {
            anyhow::bail!("not path-wrapped: {error:?}");
        };
        let cause = Some(&**cause);
        assert!(
            matches!(cause, Some(crate::Error::Settings(Error::InvalidHostColor { host, value })) if host == "Work" && value == "#1000000"),
            "{cause:?}"
        );
        assert_eq!(fs::read_to_string(&path)?, original);
        Ok(())
    }

    #[test]
    fn host_colour_write_creates_the_hosts_table_and_keeps_siblings() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("local.toml");
        fs::write(&path, "theme = 'Default'\n")?;
        for (host, color) in [
            ("Work", Some(0x5a3a3a)),
            ("Personal", Some(0x3a4a6b)),
            ("Work", None),
        ] {
            write(
                &path,
                Preference::HostColor {
                    host: host.into(),
                    color,
                },
            )?;
        }
        let text = fs::read_to_string(&path)?;
        let config = Config::parse(&text)?;
        assert_eq!(config.theme, "Default");
        assert_eq!(config.sidebar_style.hosts.len(), 1);
        assert_eq!(config.sidebar_style.hosts.get("Personal"), Some(&0x3a4a6b));
        Ok(())
    }

    #[test]
    fn host_colour_change_keeps_its_comment() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("local.toml");
        fs::write(
            &path,
            "[sidebar.hosts]\nWork = \"#112233\" # office machine\n",
        )?;
        write(
            &path,
            Preference::HostColor {
                host: "Work".into(),
                color: Some(0x445566),
            },
        )?;
        let text = fs::read_to_string(&path)?;
        assert!(
            text.contains("Work = \"#445566\" # office machine"),
            "{text}"
        );
        let config = Config::parse(&text)?;
        assert_eq!(config.sidebar_style.hosts.get("Work"), Some(&0x445566));
        Ok(())
    }
}

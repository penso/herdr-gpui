use super::*;
use crate::config::theme::theme_key;
use std::collections::HashSet;

#[cfg(windows)]
#[test]
fn shared_windows_config_path_matches_upstream_roaming_layout() {
    let vars = [
        ("USERPROFILE", r"C:\Users\test"),
        ("APPDATA", r"C:\Roaming"),
        ("XDG_CONFIG_HOME", r"C:\xdg"),
        ("HERDR_CONFIG_PATH", r"C:\explicit.toml"),
    ];
    for (count, expected) in [
        (1, r"C:\Users\test\AppData\Roaming\herdr\config.toml"),
        (2, r"C:\Roaming\herdr\config.toml"),
        (3, r"C:\xdg\herdr\config.toml"),
        (4, r"C:\explicit.toml"),
    ] {
        assert_eq!(
            daemon_config_path(|key| vars[..count]
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).into())),
            PathBuf::from(expected)
        );
    }
}

#[test]
fn discovers_sorted_names_and_loads_in_precedence_order() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let first = temp.0.join("first");
    let second = temp.0.join("second");
    fs::create_dir(&first)?;
    fs::create_dir(&second)?;
    fs::create_dir(first.join("not-a-theme"))?;
    for name in ["zebra", "alpha", "Nord"] {
        fs::write(first.join(name), "background=112233")?;
    }
    fs::write(second.join("Alpha"), "background=445566")?;
    fs::write(second.join("zebra"), "background=445566")?;
    let directories = vec![temp.0.join("missing"), first, second];
    let config = Config {
        theme: "alpha".into(),
        ..Config::default()
    };
    // The second directory's `Alpha` spells the first one's `alpha`, which
    // loads, so it is not listed twice.
    let mut expected: Vec<&str> = Theme::BUILTIN_NAMES.to_vec();
    expected.extend(["Follow Herdr", "alpha", "zebra"]);
    expected.sort_by_cached_key(|name| (name.to_lowercase(), *name));
    assert_eq!(config.available_themes_in(&directories)?, expected);
    assert_eq!(
        config
            .theme_with_directories(false, || Ok(directories.clone()))?
            .background,
        0x112233
    );
    let builtin = Config {
        theme: "Nord".into(),
        ..Config::default()
    };
    assert_eq!(
        builtin.theme_with_directories(false, || Ok(directories))?,
        Theme::builtin("Nord").context("missing builtin")?
    );
    Ok(())
}

#[test]
fn installed_ghostty_copies_of_builtins_are_listed_once() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    // Current Ghostty names, the older lowercase spellings, and themes that
    // only look alike.
    for name in [
        "TokyoNight",
        "tokyonight-storm",
        "catppuccin-mocha",
        "rose-pine",
        "Dracula+",
        "My Theme",
    ] {
        fs::write(temp.0.join(name), "background=112233")?;
    }
    let config = Config {
        theme: "light:Nord,dark:rose-pine".into(),
        ..Config::default()
    };
    let listed = config.available_themes_in(std::slice::from_ref(&temp.0))?;
    for hidden in ["tokyonight-storm", "catppuccin-mocha"] {
        assert!(!listed.iter().any(|name| name == hidden), "{hidden} listed");
    }
    for shown in [
        "TokyoNight",
        "TokyoNight Storm",
        "Catppuccin Mocha",
        "Dracula",
        "Dracula+",
        "My Theme",
        "Rose Pine",
        // Selected under its old name, so the picker can still show it.
        "rose-pine",
    ] {
        assert!(listed.iter().any(|name| name == shown), "{shown} missing");
    }
    let keys: HashSet<_> = listed.iter().map(|name| theme_key(name)).collect();
    assert_eq!(keys.len(), listed.len() - 1, "only the selection repeats");
    // The built-in, not the installed file, is what a built-in name loads.
    let builtin = Config {
        theme: "TokyoNight".into(),
        ..Config::default()
    };
    assert_eq!(
        builtin.theme_with_directories(false, || Ok(vec![temp.0.clone()]))?,
        Theme::builtin("TokyoNight").context("missing builtin")?
    );
    Ok(())
}

#[test]
fn an_impossible_theme_directory_does_not_hide_the_others() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    fs::write(temp.0.join("Mine"), "background=112233")?;
    // A Unix `XDG_DATA_DIRS` list read as one Windows path, which Windows
    // rejects as a name rather than reporting it missing.
    let directories = [
        PathBuf::from("/usr/local/share:/usr/share").join("ghostty/themes"),
        temp.0.join("bad|name"),
        temp.0.clone(),
    ];
    let config = Config {
        theme: "Mine".into(),
        ..Config::default()
    };
    assert!(
        config
            .available_themes_in(&directories)?
            .contains(&"Mine".to_owned())
    );
    assert_eq!(
        config
            .theme_with_directories(false, || Ok(directories.to_vec()))?
            .background,
        0x112233
    );
    Ok(())
}

#[test]
fn bundled_themes_are_distinct_and_carry_their_own_colors() {
    let keys: HashSet<_> = Theme::BUILTIN_NAMES
        .iter()
        .map(|name| theme_key(name))
        .collect();
    assert_eq!(keys.len(), Theme::BUILTIN_NAMES.len());
    let default = Theme::default();
    for name in &Theme::BUILTIN_NAMES[1..] {
        let theme = Theme::builtin(name).unwrap_or_else(|| panic!("{name} does not load"));
        assert_ne!(theme.background, default.background, "{name}");
        assert_ne!(theme.palette[..16], default.palette[..16], "{name}");
    }
    let tokyo = Theme::builtin("TokyoNight").unwrap_or_default();
    assert_eq!(
        (tokyo.background, tokyo.foreground, tokyo.palette[4]),
        (0x1a1b26, 0xc0caf5, 0x7aa2f7)
    );
}

#[test]
fn discovery_includes_explicit_selection_and_reports_errors() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    for name in [
        temp.0.join("custom").to_string_lossy().into_owned(),
        "~/custom".into(),
    ] {
        let config = Config {
            theme: name.clone(),
            ..Config::default()
        };
        assert!(config.available_themes_in(&[])?.contains(&name));
    }
    let not_directory = temp.0.join("file");
    fs::write(&not_directory, "")?;
    assert!(
        Config::default()
            .available_themes_in(&[not_directory])
            .is_err()
    );
    for name in Theme::BUILTIN_NAMES {
        let config = Config {
            theme: (*name).into(),
            ..Config::default()
        };
        assert!(
            config
                .theme_with_directories(false, || Err(Error::MissingHome))
                .is_ok()
        );
    }
    Ok(())
}

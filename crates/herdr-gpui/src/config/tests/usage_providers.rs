use super::*;
use crate::config::{preferences::Preference, usage_providers::ProviderEdit};

fn scope(ids: &[&str]) -> [bool; crate::usage::PROVIDER_COUNT] {
    let mut selected = [false; crate::usage::PROVIDER_COUNT];
    for (index, provider) in crate::usage::providers().enumerate() {
        selected[index] = ids.contains(&provider.id());
    }
    selected
}

fn save(edit: ProviderEdit, path: &Path) -> anyhow::Result<Config> {
    Config::save_preference_path(Preference::UsageProviders(edit), path)?;
    Ok(Config::parse(&fs::read_to_string(path)?)?)
}

/// Settings > Status bar turns a service off and on in place, so the user's
/// comments, other `[usage]` keys, and providers survive.
#[test]
fn a_service_turns_off_and_on_without_disturbing_the_usage_table() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n[usage]\nshow = true # bar\n";
    fs::write(&path, original)?;

    let config = save(ProviderEdit::Disable("cursor"), &path)?;
    assert_eq!(config.usage.hide_providers, ["cursor"]);
    let config = save(ProviderEdit::Disable("openrouter"), &path)?;
    assert_eq!(config.usage.hide_providers, ["cursor", "openrouter"]);
    // Turning off what is already off changes nothing.
    let config = save(ProviderEdit::Disable("cursor"), &path)?;
    assert_eq!(config.usage.hide_providers, ["cursor", "openrouter"]);

    let config = save(ProviderEdit::Enable("cursor"), &path)?;
    assert_eq!(config.usage.hide_providers, ["openrouter"]);
    let config = save(ProviderEdit::Enable("openrouter"), &path)?;
    assert!(config.usage.hide_providers.is_empty());
    // Nothing left hidden removes the key rather than leaving `[]` behind.
    let text = fs::read_to_string(&path)?;
    assert!(!text.contains("hide_providers"), "{text}");
    assert!(text.contains("theme = 'Nord' # keep") && text.contains("show = true # bar"));
    assert!(config.usage.show);
    Ok(())
}

#[test]
fn bulk_changes_only_displayed_services_and_preserves_unseen_choices() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    fs::write(
        &path,
        "[usage]\nshow_providers = ['cursor']\nhide_providers = ['cursor', 'from-the-future']\n",
    )?;

    let selected = scope(&["codex", "claude"]);
    for (edit, expected) in [
        (
            ProviderEdit::DisableMany(selected),
            vec!["cursor", "from-the-future", "codex", "claude"],
        ),
        (
            ProviderEdit::Enable("claude"),
            vec!["cursor", "from-the-future", "codex"],
        ),
        (
            ProviderEdit::EnableMany(selected),
            vec!["cursor", "from-the-future"],
        ),
        (
            ProviderEdit::DisableMany([false; crate::usage::PROVIDER_COUNT]),
            vec!["cursor", "from-the-future"],
        ),
    ] {
        save(edit, &path)?;
        let table: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
        let hidden: Vec<String> = table["usage"]["hide_providers"].clone().try_into()?;
        assert_eq!(hidden, expected);
        let shown: Vec<String> = table["usage"]["show_providers"].clone().try_into()?;
        assert_eq!(shown, ["cursor"]);
    }
    Ok(())
}

/// A config written by a newer build may name services this one lacks. Those
/// ids stay in the file, even through "turn all on".
#[test]
fn unknown_services_are_kept_when_the_known_ones_change() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    fs::write(&path, "[usage]\nhide_providers = ['from-the-future']\n")?;
    save(ProviderEdit::Disable("claude"), &path)?;
    let table: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
    let hidden = table["usage"]["hide_providers"]
        .as_array()
        .context("hide_providers must be a list")?;
    assert_eq!(
        hidden
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>(),
        ["from-the-future", "claude"]
    );
    save(ProviderEdit::EnableMany(scope(&["claude"])), &path)?;
    let table: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
    assert_eq!(
        table["usage"]["hide_providers"][0].as_str(),
        Some("from-the-future")
    );
    assert_eq!(
        table["usage"]["hide_providers"].as_array().map(Vec::len),
        Some(1)
    );
    Ok(())
}

#[test]
fn malformed_usage_tables_are_rejected_and_left_untouched() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    for (original, expected) in [
        ("usage = false\n", Error::InvalidUsageTable),
        (
            "[usage]\nhide_providers = 'claude'\n",
            Error::InvalidHideProviders,
        ),
        (
            "[usage]\nhide_providers = ['claude', 3]\n",
            Error::InvalidHideProviders,
        ),
    ] {
        fs::write(&path, original)?;
        let error = Config::save_preference_path(
            Preference::UsageProviders(ProviderEdit::Disable("codex")),
            &path,
        )
        .err()
        .context("malformed usage settings must be rejected")?;
        let Error::Path { source, .. } = &error else {
            anyhow::bail!("expected a path-scoped settings error: {error:?}");
        };
        assert_eq!(
            std::mem::discriminant(source.as_ref()),
            std::mem::discriminant(&expected)
        );
        assert_eq!(fs::read_to_string(&path)?, original);
    }
    Ok(())
}

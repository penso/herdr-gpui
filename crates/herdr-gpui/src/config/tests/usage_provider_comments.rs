use super::*;
use crate::config::{preferences::Preference, usage_providers::ProviderEdit};

const SIBLINGS: &str = "theme = 'Nord' # theme note\n[usage]\nshow = true # bar note\nshow_providers = ['cursor'] # scope note\n";
const ARRAY: &str = "hide_providers = [\n  # unknown prefix\n  'from-the-future' # unknown suffix\n  ,\n  # cursor prefix\n  \"cursor\" # cursor suffix\n  ,\n  # list note before selected service\n  'claude' # selected suffix\n  ,\n  # array footer\n] # provider visibility\n";
const OTHER_TABLE: &str = "[editor]\ncommand = 'vim' # editor note\n";

fn scope(ids: &[&str]) -> [bool; crate::usage::PROVIDER_COUNT] {
    let mut selected = [false; crate::usage::PROVIDER_COUNT];
    for (index, provider) in crate::usage::providers().enumerate() {
        selected[index] = ids.contains(&provider.id());
    }
    selected
}

fn save_and_reload(edit: ProviderEdit, path: &Path, expected: &[&str]) -> anyhow::Result<String> {
    Config::save_preference_path(Preference::UsageProviders(edit), path)?;
    let text = fs::read_to_string(path)?;
    let config = Config::parse(&text)?;
    let known: Vec<_> = expected
        .iter()
        .copied()
        .filter(|id| crate::usage::providers().any(|provider| provider.id() == *id))
        .collect();
    assert_eq!(config.usage.hide_providers, known);
    // Check the serialized TOML too, including ids unknown to this build.
    let table: toml::Table = toml::from_str(&text)?;
    let hidden: Vec<String> = table["usage"]["hide_providers"].clone().try_into()?;
    assert_eq!(hidden, expected);
    Ok(text)
}

fn assert_original_comments_and_siblings(text: &str) {
    assert!(text.starts_with(SIBLINGS), "{text}");
    assert!(text.ends_with(OTHER_TABLE), "{text}");
    for fragment in [
        "\n  # unknown prefix\n  'from-the-future' # unknown suffix\n  ,",
        "\n  # cursor prefix\n  \"cursor\" # cursor suffix\n  ,",
        "# list note before selected service",
        "# selected suffix",
        "# array footer\n] # provider visibility",
    ] {
        assert!(text.contains(fragment), "missing {fragment:?}: {text}");
    }
}

#[test]
fn individual_edits_preserve_internal_comments_and_unknown_value_decorations() -> anyhow::Result<()>
{
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    fs::write(&path, format!("{SIBLINGS}{ARRAY}{OTHER_TABLE}"))?;

    let text = save_and_reload(
        ProviderEdit::Disable("codex"),
        &path,
        &["from-the-future", "cursor", "claude", "codex"],
    )?;
    assert_original_comments_and_siblings(&text);
    let unchanged = save_and_reload(
        ProviderEdit::Disable("codex"),
        &path,
        &["from-the-future", "cursor", "claude", "codex"],
    )?;
    assert_eq!(unchanged, text);

    let text = save_and_reload(
        ProviderEdit::Enable("claude"),
        &path,
        &["from-the-future", "cursor", "codex"],
    )?;
    assert_original_comments_and_siblings(&text);
    Ok(())
}

#[test]
fn scoped_bulk_edits_preserve_comments_and_unselected_or_unknown_ids() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    fs::write(&path, format!("{SIBLINGS}{ARRAY}{OTHER_TABLE}"))?;
    let selected = scope(&["codex", "claude"]);

    let text = save_and_reload(
        ProviderEdit::DisableMany(selected),
        &path,
        &["from-the-future", "cursor", "claude", "codex"],
    )?;
    assert_original_comments_and_siblings(&text);
    let text = save_and_reload(
        ProviderEdit::EnableMany(selected),
        &path,
        &["from-the-future", "cursor"],
    )?;
    assert_original_comments_and_siblings(&text);
    let unchanged = save_and_reload(
        ProviderEdit::EnableMany([false; crate::usage::PROVIDER_COUNT]),
        &path,
        &["from-the-future", "cursor"],
    )?;
    assert_eq!(unchanged, text);
    Ok(())
}

#[test]
fn adding_after_a_commented_last_value_without_a_trailing_comma_is_valid() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    fs::write(
        &path,
        format!(
            "{SIBLINGS}hide_providers = [\n  # list prefix\n  'from-the-future', # unknown line note\n  'cursor' # last-value suffix\n]\n{OTHER_TABLE}"
        ),
    )?;
    let text = save_and_reload(
        ProviderEdit::Disable("claude"),
        &path,
        &["from-the-future", "cursor", "claude"],
    )?;
    assert!(text.starts_with(SIBLINGS) && text.ends_with(OTHER_TABLE));
    assert!(text.contains("# list prefix\n  'from-the-future', # unknown line note\n  'cursor' # last-value suffix\n"), "{text}");
    let text = save_and_reload(
        ProviderEdit::Enable("cursor"),
        &path,
        &["from-the-future", "claude"],
    )?;
    assert!(text.contains("# unknown line note") && text.contains("# last-value suffix"));
    Ok(())
}

#[test]
fn removing_the_last_id_keeps_comments_in_an_empty_array() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    fs::write(
        &path,
        format!(
            "{SIBLINGS}hide_providers = [\n  # list-wide note\n  'claude' # suffix note\n  ,\n  # trailing note\n]\n{OTHER_TABLE}"
        ),
    )?;
    let text = save_and_reload(ProviderEdit::Enable("claude"), &path, &[])?;
    assert!(text.starts_with(SIBLINGS) && text.ends_with(OTHER_TABLE));
    for comment in ["# list-wide note", "# suffix note", "# trailing note"] {
        assert!(text.contains(comment), "{text}");
    }
    let text = save_and_reload(ProviderEdit::Disable("cursor"), &path, &["cursor"])?;
    for comment in ["# list-wide note", "# suffix note", "# trailing note"] {
        assert!(text.contains(comment), "{text}");
    }
    Ok(())
}

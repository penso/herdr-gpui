use super::*;
use crate::config::preferences::Preference;

/// Settings > Plugins turns `[usage] inline` back on in place, keeping the
/// table's other switches, providers, and comments.
#[test]
fn usage_inline_saves_in_place_and_keeps_other_usage_settings() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n[usage]\nshow = true # bar\ninline = false # rows\nhide_providers = ['claude']\n";
    fs::write(&path, original)?;
    Config::save_preference_path(Preference::UsageInline(true), &path)?;
    assert_eq!(
        fs::read_to_string(&path)?,
        original.replace("inline = false", "inline = true")
    );
    let config = Config::parse(&fs::read_to_string(&path)?)?;
    assert!(config.usage.inline && config.usage.show);
    assert_eq!(config.usage.hide_providers, ["claude"]);

    fs::write(&path, "theme = 'Nord'\n")?;
    Config::save_preference_path(Preference::UsageInline(false), &path)?;
    assert!(!Config::parse(&fs::read_to_string(&path)?)?.usage.inline);
    Ok(())
}

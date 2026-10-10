use super::*;
use crate::config::preferences::Preference;

#[test]
fn update_channel_defaults_to_stable_and_rejects_unknown_channels() -> anyhow::Result<()> {
    assert_eq!(Config::parse("")?.updates.channel, UpdateChannel::Stable);
    assert_eq!(
        Config::parse("[updates]\nchannel = 'beta'\n")?
            .updates
            .channel,
        UpdateChannel::Beta
    );
    assert_eq!(
        Config::parse("[updates]\nchannel = 'stable'\n")?
            .updates
            .channel,
        UpdateChannel::Stable
    );
    for text in [
        "[updates]\nchannel = 'nightly'\n",
        "[updates]\nchannel = true\n",
    ] {
        assert!(Config::parse(text).is_err(), "{text}");
    }
    Ok(())
}

/// Settings toggles the channel in place, keeping comments and other keys.
#[test]
fn update_channel_saves_in_place() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n[updates]\nchannel = 'stable' # channel\n";
    fs::write(&path, original)?;
    Config::save_preference_path(Preference::UpdateChannel(UpdateChannel::Beta), &path)?;
    assert_eq!(
        fs::read_to_string(&path)?,
        original.replace("'stable'", "\"beta\"")
    );
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?.updates.channel,
        UpdateChannel::Beta
    );

    fs::write(&path, "theme = 'Nord'\n")?;
    Config::save_preference_path(Preference::UpdateChannel(UpdateChannel::Stable), &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?.updates.channel,
        UpdateChannel::Stable
    );
    Ok(())
}

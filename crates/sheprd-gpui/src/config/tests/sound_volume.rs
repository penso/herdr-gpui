use super::*;
use crate::config::preferences::Preference;

#[test]
fn sound_volume_defaults_below_full_and_reads_the_gui_config() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let gui = temp.0.join("config-gpui.toml");
    let local = gui.with_extension("local.toml");
    let daemon = temp.0.join("config.toml");
    fs::write(&gui, "")?;
    assert_eq!(Config::load_path(&gui, &daemon)?.sound.volume, 50);
    for volume in [0, 35, 100] {
        fs::write(&local, format!("[sound]\nvolume = {volume}\n"))?;
        assert_eq!(Config::load_path(&gui, &daemon)?.sound.volume, volume);
    }
    // Herdr's [ui.sound] lives in the daemon config and has no volume to share.
    fs::write(&daemon, "[ui.sound]\nenabled = false\n")?;
    assert_eq!(Config::load_path(&gui, &daemon)?.sound.volume, 100);
    Ok(())
}

#[test]
fn sound_volume_refuses_values_outside_the_range() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let gui = temp.0.join("config-gpui.toml");
    let local = gui.with_extension("local.toml");
    let daemon = temp.0.join("config.toml");
    fs::write(&gui, "")?;
    for value in ["101", "256", "-1", "0.5", "'loud'"] {
        fs::write(&local, format!("[sound]\nvolume = {value}\n"))?;
        assert!(
            Config::load_path(&gui, &daemon).is_err(),
            "volume = {value} loaded"
        );
    }
    fs::write(&local, "[sound]\ngain = 1\n")?;
    assert!(Config::load_path(&gui, &daemon).is_err());
    Ok(())
}

#[test]
fn sound_volume_preference_saves_in_place_and_refuses_out_of_range() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    let original = "theme = 'Nord' # keep\n[sound]\nvolume = 80 # mine\n";
    fs::write(&path, original)?;
    Config::save_preference_path(Preference::SoundVolume(25), &path)?;
    let saved = fs::read_to_string(&path)?;
    assert_eq!(saved, original.replace("volume = 80", "volume = 25"));
    assert_eq!(Config::parse(&saved)?.sound.volume, 25);

    let error = Config::save_preference_path(Preference::SoundVolume(101), &path)
        .err()
        .context("volume over 100 must be refused")?;
    assert!(matches!(&error, crate::Error::Path { source, .. }
        if matches!(**source, crate::Error::Settings(Error::InvalidSoundVolume))));
    assert_eq!(fs::read_to_string(&path)?, saved);
    Ok(())
}

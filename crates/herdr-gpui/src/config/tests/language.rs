use super::*;

/// Settings > General > Language writes the key `herdr-gpui language set`
/// does, in place, and reads back as what was picked.
#[test]
fn the_language_preference_writes_the_setting_in_place() -> anyhow::Result<()> {
    use crate::config::preferences::Preference;
    use herdr_i18n::Language;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(&path, "# keep\nfuture = 1\nlanguage = 'system' # mine\n")?;
    let hebrew = Language::Locale("he".parse()?);
    Config::save_preference_path(Preference::Language(hebrew.clone()), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(
        text.contains("# keep\nfuture = 1\nlanguage = \"he\" # mine"),
        "{text}"
    );
    assert_eq!(Config::parse(&text)?.language, hebrew);
    Config::save_preference_path(Preference::Language(Language::System), &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?.language,
        Language::System
    );
    Ok(())
}

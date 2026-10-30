use super::*;
use crate::config::preferences::Preference;

#[test]
fn favorite_themes_load_once_each_in_their_order() -> anyhow::Result<()> {
    assert!(Config::parse("")?.favorite_themes.is_empty());
    let config = Config::parse("favorite_themes = ['Nord', 'Dracula', 'Nord']\n")?;
    assert_eq!(config.favorite_themes, ["Nord", "Dracula"]);
    assert!(config.unknown_keys.is_empty());
    assert!(Config::parse("favorite_themes = 'Nord'\n").is_err());
    Ok(())
}

/// Starring writes the list in place, keeping the rest of the file, and
/// unstarring the last theme removes the key.
#[test]
fn the_favorite_themes_preference_writes_and_clears_the_list() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(&path, "# keep\ntheme = 'Nord' # mine\n")?;
    let starred = vec!["Nord".to_owned(), "Rosé Pine".to_owned()];
    Config::save_preference_path(Preference::FavoriteThemes(starred.clone()), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# keep\ntheme = 'Nord' # mine\n"), "{text}");
    let config = Config::parse(&text)?;
    assert_eq!(config.favorite_themes, starred);
    assert_eq!(config.theme, "Nord");
    Config::save_preference_path(Preference::FavoriteThemes(Vec::new()), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(!text.contains("favorite_themes"), "{text}");
    assert!(Config::parse(&text)?.favorite_themes.is_empty());
    Ok(())
}

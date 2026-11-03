use super::*;
use crate::config::favorite_themes::FavoriteEdit;

fn star(name: &str, starred: bool) -> FavoriteEdit {
    FavoriteEdit {
        name: name.into(),
        starred,
    }
}

#[test]
fn favorite_themes_load_once_each_in_their_order() -> anyhow::Result<()> {
    assert!(Config::parse("")?.favorite_themes.is_empty());
    let config = Config::parse("favorite_themes = ['Nord', 'Dracula', 'Nord']\n")?;
    assert_eq!(config.favorite_themes, ["Nord", "Dracula"]);
    assert!(config.unknown_keys.is_empty());
    assert!(Config::parse("favorite_themes = 'Nord'\n").is_err());
    Ok(())
}

/// Stars apply to the list the file holds when they are written, so a theme
/// another window starred meanwhile stays; unstarring the last theme removes
/// the key, and the rest of the file is kept.
#[test]
fn star_edits_apply_to_the_files_latest_list() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(
        &path,
        "# keep\ntheme = 'Nord' # mine\nfavorite_themes = ['Dracula', 'Dracula'] # stars\n",
    )?;
    let saved = Config::save_favorite_themes_path(
        &[
            star("Nord", true),
            star("Rosé Pine", true),
            star("Nord", false),
        ],
        &path,
    )?;
    assert_eq!(saved, ["Dracula", "Rosé Pine"]);
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# keep\ntheme = 'Nord' # mine\n"), "{text}");
    assert!(text.contains("] # stars"), "{text}");
    let config = Config::parse(&text)?;
    assert_eq!(config.favorite_themes, saved);
    assert_eq!(config.theme, "Nord");
    // A star for a theme already starred, or an unstar for one that is not,
    // changes nothing.
    let saved =
        Config::save_favorite_themes_path(&[star("Dracula", true), star("Missing", false)], &path)?;
    assert_eq!(saved, ["Dracula", "Rosé Pine"]);
    let saved = Config::save_favorite_themes_path(
        &[star("Dracula", false), star("Rosé Pine", false)],
        &path,
    )?;
    assert!(saved.is_empty());
    let text = fs::read_to_string(&path)?;
    assert!(!text.contains("favorite_themes"), "{text}");
    Ok(())
}

#[test]
fn a_malformed_favorites_list_is_refused_and_left_as_it_is() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    for original in [
        "favorite_themes = 'Nord'\n",
        "favorite_themes = ['Nord', 3]\n",
    ] {
        fs::write(&path, original)?;
        let Err(error) = Config::save_favorite_themes_path(&[star("Dracula", true)], &path) else {
            anyhow::bail!("{original:?} was accepted");
        };
        assert!(
            matches!(
                &error,
                crate::Error::Path { path: failed, source }
                    if *failed == path
                        && matches!(**source, crate::Error::Settings(Error::InvalidFavoriteThemes))
            ),
            "{error:?}"
        );
        assert_eq!(fs::read_to_string(&path)?, original);
    }
    Ok(())
}

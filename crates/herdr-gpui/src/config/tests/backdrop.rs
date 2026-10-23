use super::*;
use crate::config::backdrop::Edit;

/// A path absolute on every platform, which a Unix root path is not on
/// Windows.
fn picture() -> PathBuf {
    env::temp_dir().join("sea.jpg")
}

#[test]
fn the_backdrop_defaults_to_an_opaque_window() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        assert_eq!(config.backdrop, Backdrop::default());
        assert!(!config.backdrop.translucent());
        assert_eq!(config.backdrop.image_path(), None);
    }
    Ok(())
}

#[test]
fn backdrop_keys_parse_and_bad_values_are_refused() -> anyhow::Result<()> {
    // A literal string, so a Windows path's backslashes stay as they are.
    let config = Config::parse(&format!(
        "[backdrop]\nopacity = 0.8\nblur = true\nimage = '{}'\nfit = 'tile'\n",
        picture().display()
    ))?;
    assert_eq!(
        config.backdrop,
        Backdrop {
            opacity: 0.8,
            blur: true,
            image: Some(picture()),
            material: Material::Acrylic,
        }
    );
    assert!(config.backdrop.translucent());
    assert_eq!(config.unknown_keys, ["backdrop.fit"]);
    for (invalid, expected) in [
        ("[backdrop]\nopacity = 1.5\n", "opacity"),
        ("[backdrop]\nopacity = -0.1\n", "opacity"),
        ("[backdrop]\nopacity = nan\n", "opacity"),
        ("[backdrop]\nimage = ''\n", "empty"),
        ("[backdrop]\nimage = 'pictures/sea.jpg'\n", "relative"),
    ] {
        let error = Config::parse(invalid).err().context(invalid)?;
        let matched = match &error {
            crate::Error::Backdrop(crate::backdrop::Error::Opacity) => "opacity",
            crate::Error::Backdrop(crate::backdrop::Error::EmptyImage) => "empty",
            crate::Error::Backdrop(crate::backdrop::Error::RelativeImage) => "relative",
            _ => "other",
        };
        assert_eq!(matched, expected, "{invalid}: {error}");
    }
    assert!(Config::parse("[backdrop]\nblur = 'yes'\n").is_err());
    Ok(())
}

#[test]
fn a_backdrop_image_under_home_resolves_against_it() -> anyhow::Result<()> {
    let config = Config::parse("[backdrop]\nimage = '~/Pictures/sea.jpg'\n")?;
    assert_eq!(
        config.backdrop.image_path(),
        Some(home()?.join("Pictures/sea.jpg"))
    );
    Ok(())
}

#[test]
fn backdrop_edits_round_trip_and_keep_the_rest_of_the_file() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(
        &path,
        "# mine\nlayout = 'orca'\n[backdrop]\nopacity = 0.9 # how solid\nfuture = 1\n",
    )?;
    Config::save_backdrop_path(&Edit::Opacity(0.700_000_1), &path)?;
    Config::save_backdrop_path(&Edit::Blur(true), &path)?;
    Config::save_backdrop_path(&Edit::Image(Some(picture())), &path)?;
    Config::save_backdrop_path(&Edit::Material(Material::Mica), &path)?;
    Config::save_backdrop_path(&Edit::Material(Material::Acrylic), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(
        text.contains("# mine") && text.contains("future = 1"),
        "{text}"
    );
    assert!(text.contains("opacity = 0.7 # how solid"), "{text}");
    let config = Config::parse(&text)?;
    assert_eq!(
        config.backdrop,
        Backdrop {
            opacity: 0.7,
            blur: true,
            image: Some(picture()),
            material: Material::Acrylic,
        }
    );
    Config::save_backdrop_path(&Edit::Image(None), &path)?;
    assert_eq!(
        Config::parse(&fs::read_to_string(&path)?)?.backdrop.image,
        None
    );
    // A refused edit leaves the file as it was.
    let before = fs::read_to_string(&path)?;
    for edit in [
        Edit::Opacity(f32::NAN),
        Edit::Opacity(2.),
        Edit::Image(Some(PathBuf::new())),
        Edit::Image(Some("relative.png".into())),
    ] {
        assert!(
            Config::save_backdrop_path(&edit, &path).is_err(),
            "{edit:?}"
        );
    }
    assert_eq!(fs::read_to_string(&path)?, before);
    Ok(())
}

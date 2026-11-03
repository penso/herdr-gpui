use super::*;
use std::fs;

#[test]
fn reading_a_folder_counts_only_mod_files() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("mod.toml"), MANIFEST).unwrap();
    fs::write(folder.join("main.luau"), "return 1").unwrap();
    fs::write(folder.join("parts.luau"), "return 2").unwrap();
    fs::write(folder.join("README.md"), "ignored").unwrap();
    fs::write(folder.join(".hidden.luau"), "ignored").unwrap();
    fs::create_dir(folder.join("sub")).unwrap();
    fs::write(folder.join("sub/inner.luau"), "ignored").unwrap();
    let sources = Sources::read(&folder).unwrap();
    let modules = sources.modules().map(|(stem, _)| stem).collect::<Vec<_>>();
    assert_eq!(modules, ["main", "parts"]);

    let before = sources.hash.clone();
    fs::write(folder.join("README.md"), "changed").unwrap();
    assert_eq!(Sources::read(&folder).unwrap().hash, before);
    fs::write(folder.join("parts.luau"), "return 3").unwrap();
    assert_ne!(Sources::read(&folder).unwrap().hash, before);
}

#[cfg(unix)]
#[test]
fn links_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("demo");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("mod.toml"), MANIFEST).unwrap();
    fs::write(folder.join("main.luau"), "return 1").unwrap();
    fs::write(dir.path().join("secret.luau"), "return 'secret'").unwrap();
    std::os::unix::fs::symlink(dir.path().join("secret.luau"), folder.join("secret.luau")).unwrap();
    let sources = Sources::read(&folder).unwrap();
    assert!(sources.module("secret").is_none());
}

#[test]
fn the_folder_must_be_named_after_the_mod() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("other");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("mod.toml"), MANIFEST).unwrap();
    fs::write(folder.join("main.luau"), "return 1").unwrap();
    let error = Sources::read(&folder).unwrap_err();
    assert!(
        matches!(error, Error::Manifest { ref at, .. } if at == "name"),
        "{error:?}"
    );
}

#[test]
fn the_hash_covers_names_as_well_as_text() {
    let a = Sources::new(files(&[
        ("mod.toml", MANIFEST),
        ("main.luau", ""),
        ("ab.luau", "c"),
    ]));
    let b = Sources::new(files(&[
        ("mod.toml", MANIFEST),
        ("main.luau", ""),
        ("a.luau", "bc"),
    ]));
    assert_ne!(a.unwrap().hash, b.unwrap().hash);
}

#[test]
fn required_files_and_bounds() {
    let missing = Sources::new(files(&[("main.luau", "")])).unwrap_err();
    assert!(matches!(missing, Error::NoManifest), "{missing:?}");
    let missing = Sources::new(files(&[("mod.toml", MANIFEST)])).unwrap_err();
    assert!(matches!(missing, Error::NoMain), "{missing:?}");
    let large = "x".repeat(MAX_FILE_BYTES as usize + 1);
    let error = Sources::new(files(&[("mod.toml", MANIFEST), ("main.luau", &large)])).unwrap_err();
    assert!(
        matches!(
            error,
            Error::Source {
                problem: Problem::TooLarge { .. },
                ..
            }
        ),
        "{error:?}"
    );
    let error = Sources::new(files(&[
        ("mod.toml", MANIFEST),
        ("main.luau", ""),
        ("Bad.luau", ""),
    ]))
    .unwrap_err();
    assert!(
        matches!(error, Error::Source { ref file, .. } if file == "Bad.luau"),
        "{error:?}"
    );
}

#[test]
fn the_activity_icon_must_exist() {
    let manifest = "name = \"demo\"\napi = 1\n[activity]\ntitle = \"Demo\"\nicon = \"icon.svg\"\n";
    let error = Sources::new(files(&[("mod.toml", manifest), ("main.luau", "")])).unwrap_err();
    assert!(matches!(error, Error::Manifest { ref at, .. } if at == "activity.icon"));
    let sources = Sources::new(files(&[
        ("mod.toml", manifest),
        ("main.luau", ""),
        ("icon.svg", "<svg/>"),
    ]))
    .unwrap();
    assert_eq!(sources.icon("icon.svg"), Some("<svg/>"));
}

#[test]
fn the_stamp_changes_with_counted_files_only() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.luau"), "return 1").unwrap();
    let before = Stamp::take(dir.path()).unwrap();
    fs::write(dir.path().join("notes.md"), "ignored").unwrap();
    assert_eq!(Stamp::take(dir.path()).unwrap(), before);
    fs::write(dir.path().join("main.luau"), "return 12").unwrap();
    assert_ne!(Stamp::take(dir.path()).unwrap(), before);
}

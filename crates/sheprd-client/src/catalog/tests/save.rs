use super::*;

fn host(id: char, target: &str) -> SavedHost {
    SavedHost {
        id: id.to_string().repeat(32),
        label: format!("Device {target}"),
        target: target.into(),
        session: "default".into(),
        enabled: true,
    }
}

#[test]
fn saves_a_device_without_changing_existing_profiles_or_selection() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    let first = host('a', "first");
    assert_eq!(store_host(&path, &first).unwrap(), first);
    let mut catalog = load_catalog(&path).unwrap();
    catalog.selected_profile = Some(first.id.clone());
    fs::write(&path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    store_selection(&path, Some(&first.id)).unwrap();
    let selection_path = path.with_file_name("endpoint-selection.json");
    let selection = fs::read(&selection_path).unwrap();

    let second = host('b', "ssh://dev@second:2222");
    assert_eq!(store_host(&path, &second).unwrap(), second);
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded.ssh, vec![first.clone(), second]);
    assert_eq!(loaded.selected_profile, Some(first.id));
    assert_eq!(fs::read(&selection_path).unwrap(), selection);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn duplicate_target_and_session_keeps_the_original_profile() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    let original = host('a', "box");
    store_host(&path, &original).unwrap();
    let before = fs::read(&path).unwrap();
    let mut duplicate = host('b', "box");
    duplicate.label = "Replacement".into();
    assert_eq!(store_host(&path, &duplicate).unwrap(), original);
    assert_eq!(fs::read(&path).unwrap(), before);
    duplicate.session = "work".into();
    assert_eq!(store_host(&path, &duplicate).unwrap(), duplicate);
    assert_eq!(load_path(&path).unwrap(), vec![original, duplicate]);
}

#[test]
fn invalid_additions_preserve_the_catalog() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    store_host(&path, &host('a', "first")).unwrap();
    let before = fs::read(&path).unwrap();
    let mut bad_label = host('b', "second");
    bad_label.label = "\n".into();
    let mut bad_session = host('c', "third");
    bad_session.session = "../escape".into();
    for invalid in [
        bad_label,
        bad_session,
        host('z', "invalid-id"),
        host('a', "duplicate-id"),
        host('d', "ssh://user:secret@box"),
    ] {
        let error = store_host(&path, &invalid).unwrap_err();
        assert!(matches!(error, Error::Storage { .. }));
        assert!(std::error::Error::source(&error).is_some());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn malformed_and_full_catalogs_are_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    for bytes in [b"not json".to_vec(), vec![b' '; 65537]] {
        fs::write(&path, &bytes).unwrap();
        assert!(store_host(&path, &host('a', "box")).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    let catalog = Catalog {
        version: 1,
        selected_profile: None,
        ssh: (0..64)
            .map(|index| SavedHost {
                id: format!("{index:032x}"),
                ..host('a', &format!("box-{index}"))
            })
            .collect(),
    };
    let before = serde_json::to_vec(&catalog).unwrap();
    fs::write(&path, &before).unwrap();
    assert!(store_host(&path, &host('f', "one-too-many")).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn concurrent_saves_of_different_devices_keep_both() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    let first = host('a', "first");
    let second = host('b', "second");
    std::thread::scope(|scope| {
        let one = scope.spawn(|| store_host(&path, &first).unwrap());
        let two = scope.spawn(|| store_host(&path, &second).unwrap());
        assert_eq!(one.join().unwrap(), first);
        assert_eq!(two.join().unwrap(), second);
    });
    let mut hosts = load_path(&path).unwrap();
    hosts.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(hosts, vec![first, second]);
}

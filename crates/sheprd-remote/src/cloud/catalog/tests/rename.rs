use super::*;

#[test]
fn renaming_changes_only_the_label_and_a_missing_device_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    save_in(&path, device("w1")).unwrap();
    save_in(&path, device("w2")).unwrap();
    rename_in(&path, CloudProvider::ALL[0], "w2", "Build box").unwrap();
    let saved = read(&path).unwrap();
    assert_eq!(saved[0], device("w1"));
    assert_eq!(
        saved[1],
        SavedDevice {
            label: "Build box".into(),
            ..device("w2")
        }
    );
    assert!(matches!(
        rename_in(&path, CloudProvider::ALL[0], "gone", "x"),
        Err(Error::Missing)
    ));
    // The file's own checks still apply: a label is never written empty.
    assert!(rename_in(&path, CloudProvider::ALL[0], "w1", " ").is_err());
    assert_eq!(read(&path).unwrap()[0], device("w1"));
}

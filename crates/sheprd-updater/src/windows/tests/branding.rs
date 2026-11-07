use super::*;

#[test]
fn both_executable_names_are_eligible_only_in_the_existing_installation() {
    let local = tempfile::tempdir().unwrap();
    let installed = local.path().join("Programs/Herdr UI");
    fs::create_dir_all(&installed).unwrap();
    for name in ["herdr-gpui.exe", "sheprd.exe"] {
        let executable = installed.join(name);
        fs::write(&executable, b"app").unwrap();
        assert!(installed_in(&executable, local.path()));
        let other = local.path().join(name);
        fs::write(&other, b"app").unwrap();
        assert!(!installed_in(&other, local.path()));
    }
    let renamed_directory = local.path().join("Programs/Sheprd");
    fs::create_dir_all(&renamed_directory).unwrap();
    let other = renamed_directory.join("sheprd.exe");
    fs::write(&other, b"app").unwrap();
    assert!(!installed_in(&other, local.path()));
}

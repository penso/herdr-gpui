use super::*;

#[test]
fn bundled_helpers_precede_path_copies_for_both_name_generations() {
    let bundle = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let exe = bundle.path().join("sheprd");
    let path = std::env::join_paths([external.path()]).unwrap();
    for name in ["acp", "automate", "nearby", "remote-control"] {
        for extension in ["", ".exe"] {
            let legacy = format!("herdr-{name}{extension}");
            let renamed = format!("sheprd-{name}{extension}");
            let bundled_legacy = bundle.path().join(&legacy);
            let bundled_renamed = bundle.path().join(&renamed);
            let external_legacy = external.path().join(&legacy);
            let external_renamed = external.path().join(&renamed);
            helper(&bundled_legacy);
            helper(&external_legacy);
            helper(&external_renamed);
            assert_eq!(
                locate_in(&legacy, Some(&exe), Some(&path)),
                Some(bundled_legacy.clone())
            );
            helper(&bundled_renamed);
            assert_eq!(
                locate_in(&legacy, Some(&exe), Some(&path)),
                Some(bundled_renamed.clone())
            );
            std::fs::remove_file(bundled_legacy).unwrap();
            std::fs::remove_file(bundled_renamed).unwrap();
            assert_eq!(
                locate_in(&legacy, Some(&exe), Some(&path)),
                Some(external_renamed.clone())
            );
            std::fs::remove_file(external_renamed).unwrap();
            assert_eq!(
                locate_in(&legacy, Some(&exe), Some(&path)),
                Some(external_legacy)
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn legacy_helper_beside_real_executable_precedes_renamed_path_copy() {
    let bundle = tempfile::tempdir().unwrap();
    let links = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let real = bundle.path().join("sheprd");
    let exe = links.path().join("sheprd");
    helper(&real);
    std::os::unix::fs::symlink(&real, &exe).unwrap();
    let legacy = bundle.path().join("herdr-acp");
    helper(&legacy);
    helper(&external.path().join("sheprd-acp"));
    let path = std::env::join_paths([external.path()]).unwrap();
    assert_eq!(
        locate_in("herdr-acp", Some(&exe), Some(&path)),
        Some(legacy.canonicalize().unwrap())
    );
}

#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_canonical_folder_is_the_folder_itself() {
    let root = tempfile::tempdir().unwrap();
    let path = canonicalize(root.path()).unwrap();
    assert!(path.is_absolute());
    assert!(path.is_dir());
    // Canonical twice is canonical once, so comparisons are stable.
    assert_eq!(canonicalize(&path).unwrap(), path);
    #[cfg(windows)]
    assert!(
        !path.to_str().unwrap().starts_with(r"\\?\"),
        "{}",
        path.display()
    );
}

#[cfg(windows)]
#[test]
fn the_verbatim_prefix_goes_only_where_the_plain_path_means_the_same() {
    let plain_of = |path: &str| plain(PathBuf::from(path));
    assert_eq!(
        plain_of(r"\\?\C:\work\herdr"),
        PathBuf::from(r"C:\work\herdr")
    );
    assert_eq!(
        plain_of(r"\\?\UNC\server\share\herdr"),
        PathBuf::from(r"\\server\share\herdr")
    );
    for kept in [
        r"\\?\C:\work\trailing.",
        r"\\?\C:\work\trailing ",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\work",
        r"\\?\Volume{00000000-0000-0000-0000-000000000000}\work",
    ] {
        assert_eq!(plain_of(kept), PathBuf::from(kept), "{kept}");
    }
    let long = format!(r"\\?\C:\{}", "a".repeat(MAX_PATH));
    assert_eq!(plain_of(&long), PathBuf::from(&long));
    assert_eq!(plain_of(r"C:\work"), PathBuf::from(r"C:\work"));
}

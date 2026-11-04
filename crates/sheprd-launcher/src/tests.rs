#![allow(clippy::unwrap_used)]

use super::*;

mod compatibility;

#[test]
fn gui_is_the_launchers_sibling() {
    let directory = Path::new("install").join("dir");
    let gui = gui_path(&directory.join("herdr-gpui-launcher.exe")).unwrap();
    assert_eq!(gui, directory.join("sheprd.exe"));
}

#[test]
fn parentless_launcher_path_is_refused() {
    let error = gui_path(Path::new("")).unwrap_err();
    assert!(
        matches!(&error, Error::NoParent { launcher } if launcher.as_os_str().is_empty()),
        "{error:?}"
    );
}

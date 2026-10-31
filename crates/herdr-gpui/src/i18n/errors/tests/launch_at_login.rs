use super::*;
use crate::launch_at_login::Error;
use std::path::PathBuf;

#[test]
fn every_launch_at_login_variant_has_a_message() {
    let registry = |operation| Error::Registry {
        operation,
        source: windows_result::Error::empty(),
    };
    assert_localized(&[
        Error::NoLauncher {
            path: PathBuf::from(r"C:\Program Files\Herdr UI\herdr-gpui-launcher.exe"),
        },
        Error::CurrentExe(std::io::Error::other("disk on fire")),
        registry("open"),
        registry("read"),
        registry("write"),
        registry("remove"),
    ]);
}

use crate::remote_browser::install::SHIM;

/// The shim is written verbatim to a Unix host, where a CR left by a Windows
/// checkout turns the shebang into `/bin/sh\r` and the shim cannot run.
#[test]
fn the_shim_has_unix_line_endings_on_every_platform() {
    assert!(SHIM.starts_with("#!/bin/sh\n"));
    assert!(!SHIM.contains('\r'));
}

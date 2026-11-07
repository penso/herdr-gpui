//! `herdr-gpui-launcher.exe` is a windowed program, so Explorer and the Start menu open it
//! without a console, and it carries the Herdr icon as resource 1 and the same manifest as
//! herdr-gpui.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "../../test-support/pe.rs"]
mod pe;

use std::path::Path;

const WINDOWS_GUI_SUBSYSTEM: u16 = 2;
/// The icon resource Explorer and the taskbar show for the executable.
const ICON_RESOURCE_ID: u32 = 1;
/// The manifest Windows reads when it starts an executable.
const MANIFEST_RESOURCE_ID: u32 = 1;

fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_sheprd-launcher"))
}

#[test]
fn launcher_is_a_windowed_program() {
    assert_eq!(pe::subsystem(executable()).unwrap(), WINDOWS_GUI_SUBSYSTEM);
}

#[test]
fn launcher_embeds_the_icon_as_resource_one() {
    assert!(pe::has_resource_type(executable(), pe::RT_GROUP_ICON).unwrap());
    assert!(pe::has_icon_group(executable(), ICON_RESOURCE_ID).unwrap());
}

#[test]
fn launcher_embeds_herdr_gpuis_manifest() {
    assert_eq!(
        pe::resources(executable(), pe::RT_MANIFEST).unwrap(),
        [(
            Some(MANIFEST_RESOURCE_ID),
            include_bytes!("../../sheprd-gpui/windows.manifest").to_vec()
        )]
    );
}

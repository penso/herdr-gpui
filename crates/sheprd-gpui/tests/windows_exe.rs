//! The Windows executable carries its icon as resource 1 and Herdr's manifest
//! in place of GPUI's, and stays a console program so the CLI keeps its stdout
//! and exit codes.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "../../test-support/pe.rs"]
mod pe;

use std::{fs, io::ErrorKind, path::Path};

const CONSOLE_SUBSYSTEM: u16 = 3;
/// The icon resource GPUI's window class loads by `LoadImageW(module, 1, ...)`.
const ICON_RESOURCE_ID: u32 = 1;
/// The manifest Windows reads when it starts an executable
/// (`CREATEPROCESS_MANIFEST_RESOURCE_ID`).
const MANIFEST_RESOURCE_ID: u32 = 1;
const MANIFEST: &str = include_str!("../windows.manifest");

fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_sheprd"))
}

#[test]
fn executable_embeds_its_icon_as_resource_one() {
    assert!(pe::has_resource_type(executable(), pe::RT_GROUP_ICON).unwrap());
    assert!(pe::has_icon_group(executable(), ICON_RESOURCE_ID).unwrap());
}

#[test]
fn executable_embeds_only_herdrs_manifest() {
    assert_eq!(
        pe::resources(executable(), pe::RT_MANIFEST).unwrap(),
        [(Some(MANIFEST_RESOURCE_ID), MANIFEST.as_bytes().to_vec())]
    );
}

#[test]
fn manifest_keeps_gpuis_settings_and_adds_long_paths_and_utf8() {
    for setting in [
        r#"<requestedExecutionLevel level="asInvoker" uiAccess="false" />"#,
        r#"<supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}" />"#,
        r#"<dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>"#,
        r#"<dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>"#,
        r#"<longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>"#,
        r#"<activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>"#,
        r#"<heapType xmlns="http://schemas.microsoft.com/SMI/2020/WindowsSettings">SegmentHeap</heapType>"#,
        r#"name="Microsoft.Windows.Common-Controls""#,
    ] {
        assert_eq!(MANIFEST.matches(setting).count(), 1, "{setting}");
    }
}

#[test]
fn executable_is_a_console_program() {
    assert_eq!(pe::subsystem(executable()).unwrap(), CONSOLE_SUBSYSTEM);
}

#[test]
fn reader_rejects_garbage_and_truncated_images() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("windows-exe-reader");
    fs::create_dir_all(&dir).unwrap();
    let image = fs::read(executable()).unwrap();
    let cases = [
        ("empty", Vec::new(), ErrorKind::InvalidData),
        (
            "garbage",
            b"not a portable executable".to_vec(),
            ErrorKind::InvalidData,
        ),
        (
            "dos-header-only",
            image[..0x40].to_vec(),
            ErrorKind::UnexpectedEof,
        ),
        (
            "truncated",
            image[..0x3C].to_vec(),
            ErrorKind::UnexpectedEof,
        ),
    ];
    for (name, contents, kind) in cases {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        assert_eq!(pe::subsystem(&path).unwrap_err().kind(), kind, "{name}");
        let error = pe::has_resource_type(&path, pe::RT_MANIFEST).unwrap_err();
        assert_eq!(error.kind(), kind, "{name}");
        let error = pe::has_icon_group(&path, ICON_RESOURCE_ID).unwrap_err();
        assert_eq!(error.kind(), kind, "{name}");
        let error = pe::resources(&path, pe::RT_MANIFEST).unwrap_err();
        assert_eq!(error.kind(), kind, "{name}");
    }
}

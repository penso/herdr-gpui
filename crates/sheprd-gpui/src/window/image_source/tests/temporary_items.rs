use super::*;

/// Where macOS puts a live screenshot thumbnail while it is dragged.
fn thumbnail(name: &str) -> PathBuf {
    PathBuf::from("/var/folders/g5/abc/T/TemporaryItems/NSIRD_screencaptureui_vKTdfp").join(name)
}

#[test]
fn a_screenshot_thumbnail_is_a_temporary_drag_image() {
    let path = thumbnail("Screenshot 2026-10-16 at 2.09.54\u{202f}PM.png");
    assert!(matches!(
        temporary_drag_image(std::slice::from_ref(&path)),
        Some(Source::File { path: actual, extension: "png" }) if actual == path
    ));
    // Any app's item replacement directory, at any depth below it.
    let nested = PathBuf::from("/private/var/folders/x/T/TemporaryItems/NSIRD_Photos_1/a/b.JPEG");
    assert!(matches!(
        temporary_drag_image(&[nested]),
        Some(Source::File {
            extension: "jpg",
            ..
        })
    ));
}

#[test]
fn saved_files_other_formats_and_several_files_keep_their_paths() {
    for paths in [
        // A saved screenshot is readable by the pane's processes.
        vec![PathBuf::from("/Users/me/Desktop/Screenshot.png")],
        // Not an image the bridge carries.
        vec![thumbnail("Screenshot.tiff")],
        vec![thumbnail("notes.txt")],
        // The bridge carries one image; a multi-file drop stays a path paste.
        vec![thumbnail("a.png"), thumbnail("b.png")],
        vec![],
        // The directory itself, a look-alike name, or the wrong parent.
        vec![PathBuf::from(
            "/T/TemporaryItems/NSIRD_screencaptureui_x.png",
        )],
        vec![PathBuf::from("/T/TemporaryItems/NSIRDx/a.png")],
        vec![PathBuf::from("/T/Items/NSIRD_screencaptureui_x/a.png")],
        vec![PathBuf::from("/tmp/NSIRD_screencaptureui_x/a.png")],
        // Relative paths are never read.
        vec![PathBuf::from("TemporaryItems/NSIRD_x/a.png")],
    ] {
        assert!(temporary_drag_image(&paths).is_none(), "{paths:?}");
    }
}

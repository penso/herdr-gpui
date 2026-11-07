#![allow(clippy::unwrap_used)]
use super::*;

fn tiff(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Tiff)
        .unwrap();
    bytes.into_inner()
}

#[test]
fn screenshots_name_absolute_printable_files() {
    assert_eq!(
        screenshot_path("/tmp/page.png"),
        Some(PathBuf::from("/tmp/page.png"))
    );
    for invalid in ["", "page.png", "/tmp/pa\nge.png", "/"] {
        assert!(screenshot_path(invalid).is_none(), "{invalid:?}");
    }
}

#[test]
fn a_capture_replaces_the_file_with_a_png() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("page.png");
    std::fs::write(&path, b"old").unwrap();
    save_png(&tiff(4, 3), &path).unwrap();
    let saved = image::open(&path).unwrap();
    assert_eq!((saved.width(), saved.height()), (4, 3));
    // Nothing but the picture is left in the folder.
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    assert!(matches!(
        save_png(b"not a tiff", &path),
        Err(crate::Error::ScreenshotImage)
    ));
    let missing = folder.path().join("missing").join("page.png");
    let error = save_png(&tiff(1, 1), &missing).unwrap_err();
    assert!(matches!(&error, crate::Error::ScreenshotWrite { path, .. } if *path == missing));
    assert!(std::error::Error::source(&error).is_some());
}

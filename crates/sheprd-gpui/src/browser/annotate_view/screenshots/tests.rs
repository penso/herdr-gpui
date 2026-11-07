#![allow(clippy::unwrap_used)]
use super::{SCREENSHOT_AGE, save_screenshots_in};
use gpui::{Image, ImageFormat};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

#[test]
fn screenshots_are_private_files_and_old_ones_go() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("note-1-1.png");
    std::fs::write(&stale, b"old").unwrap();
    let week_ago = SystemTime::now() - SCREENSHOT_AGE - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(week_ago)
        .unwrap();
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, b"png bytes".to_vec()));
    let paths = save_screenshots_in(dir.path(), &[None, Some(image)], SystemTime::now()).unwrap();
    assert!(paths[0].is_none());
    let saved = paths[1].as_ref().unwrap();
    assert_eq!(std::fs::read(saved).unwrap(), b"png bytes");
    assert!(
        saved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-2.png")
    );
    assert!(!stale.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(saved), 0o600);
        assert_eq!(mode(dir.path()), 0o700);
    }
}

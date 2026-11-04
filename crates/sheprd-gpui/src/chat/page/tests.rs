#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn a_fence_is_written_alone_in_a_private_folder_named_by_what_it_holds() {
    let first = write_page(b"<h1>one</h1>").unwrap();
    let again = write_page(b"<h1>one</h1>").unwrap();
    let other = write_page(b"<h1>two</h1>").unwrap();
    assert_eq!(first, again, "the same page reuses its file");
    assert_ne!(
        first.parent(),
        other.parent(),
        "each page has its own folder"
    );
    assert_eq!(first.file_name().unwrap(), "index.html");
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "<h1>one</h1>");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(first.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "only this user reads it");
    }
    // A local page refuses a folder with a hidden name in its path.
    #[cfg(any(target_os = "macos", windows))]
    {
        let home = crate::config::home().ok();
        assert!(crate::browser::LocalFile::new(&first, home.as_deref()).is_ok());
    }
}

#[test]
fn panel_pages_never_meet_store_tabs() {
    assert!(TabId::panel(1).is_panel());
    assert!(!TabId::from(1).is_panel());
    assert!(!TabId::from(u64::from(u32::MAX)).is_panel());
    assert_ne!(TabId::panel(1), TabId::panel(2));
}

#[test]
fn an_html_file_is_copied_alone_so_its_folder_stays_out_of_reach() {
    let dir = tempfile::tempdir().unwrap();
    let page = dir.path().join("report.html");
    std::fs::write(&page, "<p>report</p>").unwrap();
    std::fs::write(dir.path().join("secret.txt"), "keys").unwrap();
    let copied = write_page(&read_page(&page).unwrap()).unwrap();
    assert_ne!(copied.parent(), page.parent());
    assert_eq!(std::fs::read_to_string(&copied).unwrap(), "<p>report</p>");
    assert!(!copied.parent().unwrap().join("secret.txt").exists());
    let big = dir.path().join("big.html");
    std::fs::write(&big, vec![b'x'; MAX_FILE as usize + 1]).unwrap();
    assert!(read_page(&big).is_err());
}

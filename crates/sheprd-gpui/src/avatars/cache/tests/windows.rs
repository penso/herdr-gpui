use super::*;
use crate::avatars::cache::windows::root_in;
use std::io::ErrorKind;

const URL: &str = "https://avatars.githubusercontent.com/u/1";

#[test]
fn the_root_is_per_user_under_local_app_data() {
    assert_eq!(
        root_in(Some(r"C:\Users\me\AppData\Local".into())),
        Some(PathBuf::from(
            r"C:\Users\me\AppData\Local\herdr\gpui\cache\avatars-v1"
        ))
    );
    for missing in [None, Some("".into()), Some(r"relative\dir".into())] {
        assert_eq!(root_in(missing), None);
    }
}

#[test]
fn corrupt_and_oversized_entries_are_removed() {
    let fixture = Fixture::new();
    let now = SystemTime::now();
    write(&fixture.0, URL, &png(), now).unwrap();
    let slot = fixture.0.join(key(URL).1);
    for bytes in [b"corrupt".to_vec(), vec![0; LIMIT + 41]] {
        std::fs::write(&slot, bytes).unwrap();
        assert!(read(&fixture.0, URL, now).is_none());
        assert!(!slot.exists());
        write(&fixture.0, URL, &png(), now).unwrap();
    }
    assert!(read(&fixture.0, URL, now).is_some());
}

#[test]
fn an_interrupted_write_is_recovered() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(&fixture.0).unwrap();
    std::fs::write(fixture.0.join("pending"), b"left over").unwrap();
    write(&fixture.0, URL, &png(), SystemTime::now()).unwrap();
    assert!(read(&fixture.0, URL, SystemTime::now()).is_some());
    assert!(!fixture.0.join("pending").exists());
}

/// Symbolic links need Developer Mode or an elevated token on Windows. Without
/// either the link cannot exist, so there is nothing to refuse.
fn link(make: std::io::Result<()>) -> bool {
    match make {
        Ok(()) => true,
        Err(error)
            if error.kind() == ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314) =>
        {
            eprintln!("skipping symbolic links: {error}");
            false
        }
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn symbolic_links_are_not_followed_or_written_through() {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    let fixture = Fixture::new();
    let now = SystemTime::now();
    write(&fixture.0, URL, &png(), now).unwrap();
    let slot = fixture.0.join(key(URL).1);
    let outside = Fixture::new();
    std::fs::create_dir_all(&outside.0).unwrap();
    let target = outside.0.join("untouched");
    std::fs::write(&target, png_entry(now)).unwrap();
    std::fs::remove_file(&slot).unwrap();
    if !link(symlink_file(&target, &slot)) {
        return;
    }
    // A link to a valid entry is still refused, and a write replaces the link.
    assert!(read(&fixture.0, URL, now).is_none());
    write(&fixture.0, URL, &png(), now).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), png_entry(now));
    assert!(!std::fs::symlink_metadata(&slot).unwrap().is_symlink());
    assert!(read(&fixture.0, URL, now).is_some());
    let linked = Fixture::new();
    symlink_dir(&fixture.0, &linked.0).unwrap();
    refused_through(&linked.0, &fixture.0, now);
}

/// Junctions need no privilege, so this runs on every machine.
#[test]
fn a_junction_to_the_cache_directory_is_refused() {
    let fixture = Fixture::new();
    let now = SystemTime::now();
    write(&fixture.0, URL, &png(), now).unwrap();
    let linked = Fixture::new();
    let status = sheprd_client::new_command("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&linked.0)
        .arg(&fixture.0)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(std::fs::symlink_metadata(&linked.0).unwrap().is_symlink());
    refused_through(&linked.0, &fixture.0, now);
}

/// Neither reads nor writes go through `linked`, a link to the cache in
/// `real`, and removing `linked` leaves `real` intact.
fn refused_through(linked: &Path, real: &Path, now: SystemTime) {
    assert!(read(linked, URL, now).is_none());
    assert!(
        write(
            linked,
            "https://avatars.githubusercontent.com/u/2",
            &png(),
            now
        )
        .is_none()
    );
    std::fs::remove_dir(linked).unwrap();
    assert!(read(real, URL, now).is_some());
    assert!(read(real, "https://avatars.githubusercontent.com/u/2", now).is_none());
}

/// What `write` stores for `URL` at `now`.
fn png_entry(now: SystemTime) -> Vec<u8> {
    let fixture = Fixture::new();
    write(&fixture.0, URL, &png(), now).unwrap();
    std::fs::read(fixture.0.join(key(URL).1)).unwrap()
}

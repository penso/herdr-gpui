#![allow(clippy::unwrap_used)]
use super::*;
use std::cell::Cell;

fn code(error: &crate::Error) -> Option<ErrorCode> {
    match error {
        crate::Error::ControlRejected { code, .. } => Some(*code),
        _ => None,
    }
}

fn unavailable() -> crate::Error {
    crate::Error::ControlUnavailable {
        path: PathBuf::from("pipe"),
        source: std::io::ErrorKind::NotFound.into(),
    }
}

#[test]
fn only_an_existing_folder_is_sent() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("project");
    std::fs::create_dir(&folder).unwrap();
    let file = root.path().join("file.txt");
    std::fs::write(&file, "x").unwrap();

    let sent = directory(&folder).unwrap();
    assert_eq!(
        Path::new(&sent),
        crate::palette::canonicalize(&folder).unwrap()
    );
    // A relative path resolves against the caller's directory.
    let relative = directory(Path::new(".")).unwrap();
    assert_eq!(
        Path::new(&relative),
        crate::palette::canonicalize(&std::env::current_dir().unwrap()).unwrap()
    );
    let error = directory(&file).unwrap_err();
    assert_eq!(code(&error), Some(ErrorCode::InvalidRequest));
    let error = directory(&root.path().join("missing")).unwrap_err();
    assert_eq!(code(&error), Some(ErrorCode::NotFound));
}

#[cfg(windows)]
#[test]
fn device_namespaces_are_refused_and_drives_and_shares_kept() {
    for path in [
        r"\\.\PhysicalDrive0",
        r"\\.\C:\Users",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1",
        r"\\?\Volume{00000000-0000-0000-0000-000000000000}\",
        r"Users\me",
    ] {
        assert!(device(Path::new(path)), "{path}");
    }
    for path in [
        r"C:\Users\me",
        r"\\?\C:\Users\me",
        r"\\server\share\project",
        r"\\?\UNC\server\share\project",
    ] {
        assert!(!device(Path::new(path)), "{path}");
    }
}

#[cfg(not(windows))]
#[test]
fn kernel_trees_are_refused() {
    assert!(device(Path::new("/dev/shm")));
    assert!(device(Path::new("/proc/self")));
    assert!(!device(Path::new("/home/me/project")));
    assert!(!device(Path::new("/devices")));
}

#[test]
fn a_starting_app_is_retried_until_it_answers() {
    let start = Instant::now();
    let attempts = Cell::new(0);
    let result = retry(
        start + START_WAIT,
        || {
            attempts.set(attempts.get() + 1);
            match attempts.get() {
                1 => Err(unavailable()),
                2 => Err(crate::Error::ControlRejected {
                    code: ErrorCode::Busy,
                    message: "not connected".into(),
                }),
                _ => Ok(Output::message("Opening")),
            }
        },
        |_| {},
        || start,
    );
    assert_eq!(result.unwrap().text, "Opening");
    assert_eq!(attempts.get(), 3);
}

#[test]
fn a_refusal_is_not_retried_and_the_wait_is_bounded() {
    let start = Instant::now();
    let attempts = Cell::new(0);
    let refused = retry(
        start + START_WAIT,
        || {
            attempts.set(attempts.get() + 1);
            Err(crate::Error::ControlRejected {
                code: ErrorCode::WindowBusy,
                message: "A dialog is open".into(),
            })
        },
        |_| {},
        || start,
    );
    assert_eq!(code(&refused.err().unwrap()), Some(ErrorCode::WindowBusy));
    assert_eq!(attempts.get(), 1);

    // Each pause moves the clock on; past the deadline the wait ends.
    let clock = Cell::new(start);
    let late = retry(
        start + START_WAIT,
        || Err(unavailable()),
        |pause| clock.set(clock.get() + pause),
        || clock.get(),
    );
    assert_eq!(code(&late.err().unwrap()), Some(ErrorCode::Timeout));
    assert!(clock.get() >= start + START_WAIT);
    assert!(clock.get() < start + START_WAIT + RETRY * 2);
}

#[cfg(windows)]
#[test]
fn a_drive_root_explorer_quoted_opens_the_root() {
    // `"C:\"` on Explorer's command line arrives as `C:"`.
    let drive = std::env::var("SystemDrive").unwrap();
    let sent = directory(Path::new(&format!("{drive}\""))).unwrap();
    assert_eq!(sent, format!("{drive}\\"));
    assert_eq!(unquoted(Path::new(r"C:\work")), Path::new(r"C:\work"));
}

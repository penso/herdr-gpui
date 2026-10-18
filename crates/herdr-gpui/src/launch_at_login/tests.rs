#![allow(clippy::unwrap_used)]
use super::*;

/// A scratch key under `HKCU\Software` standing in for `HKEY_CURRENT_USER`,
/// so tests never touch the real `Run` key. Removed on drop.
struct Scratch {
    path: String,
    key: Key,
}

impl Scratch {
    fn new() -> Self {
        let path = format!(r"Software\herdr-gpui-tests\{}", uuid::Uuid::new_v4());
        let key = CURRENT_USER.create(&path).unwrap();
        Self { path, key }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = CURRENT_USER.remove_tree(&self.path);
    }
}

#[test]
fn startup_apps_marks_decide_whether_a_present_entry_runs() {
    assert_eq!(State::of(false, None), State::Off);
    assert_eq!(State::of(false, Some(&[3; 12])), State::Off);
    assert_eq!(State::of(true, None), State::On);
    assert_eq!(State::of(true, Some(&[])), State::On);
    for on in [2, 6] {
        assert_eq!(State::of(true, Some(&[on, 0, 0, 0])), State::On);
    }
    for off in [3, 7] {
        assert_eq!(
            State::of(true, Some(&[off, 0, 0, 0])),
            State::DisabledByWindows
        );
    }
}

#[test]
fn the_command_line_quotes_a_path_with_spaces() {
    let launcher =
        Path::new(r"C:\Users\me\AppData\Local\Programs\Herdr UI\herdr-gpui-launcher.exe");
    assert_eq!(
        command_line(launcher),
        r#""C:\Users\me\AppData\Local\Programs\Herdr UI\herdr-gpui-launcher.exe""#
    );
}

#[test]
fn turning_it_on_needs_the_launcher_beside_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("herdr-gpui.exe");
    let Err(Error::NoLauncher { path }) = launcher_beside(&exe) else {
        panic!("a missing launcher must be refused");
    };
    assert_eq!(path, dir.path().join(LAUNCHER));
    std::fs::write(&path, b"").unwrap();
    assert_eq!(launcher_beside(&exe).unwrap(), path);
}

#[test]
fn an_entry_is_written_read_back_and_removed_with_its_mark() {
    let scratch = Scratch::new();
    let root = &scratch.key;
    assert_eq!(state_in(root).unwrap(), State::Off);
    // Removing what is not there, keys included, is not an error.
    set_in(root, None).unwrap();

    let launcher = Path::new(r"C:\Programs\Herdr UI\herdr-gpui-launcher.exe");
    set_in(root, Some(launcher)).unwrap();
    assert_eq!(state_in(root).unwrap(), State::On);
    assert_eq!(
        root.open(RUN).unwrap().get_string(VALUE).unwrap(),
        command_line(launcher)
    );

    // Task Manager turns it off by marking it, which the app leaves alone.
    let approved = root.create(APPROVED).unwrap();
    let mut off = [0; 12];
    off[0] = 3;
    approved
        .set_bytes(VALUE, windows_registry::Type::Bytes, &off)
        .unwrap();
    assert_eq!(state_in(root).unwrap(), State::DisabledByWindows);

    set_in(root, None).unwrap();
    assert_eq!(state_in(root).unwrap(), State::Off);
    assert!(approved.get_value(VALUE).is_err());
    // The keys themselves stay: other apps' entries live there.
    assert!(root.open(RUN).is_ok());
}

use super::*;

#[test]
fn consecutive_pastes_acquire_the_current_selection_each_time() -> Result<()> {
    let mut selection = b"first".to_vec();
    let mut calls = 0;
    for expected in ["first", "second\n"] {
        let item = read_commands(false, true, true, |program, args, limit, _| {
            calls += 1;
            assert_eq!(program, "wl-paste");
            assert_eq!(args, ["--no-newline", "--type", "text/plain;charset=utf-8"]);
            assert_eq!(limit, TEXT_LIMIT);
            Ok(Some(selection.clone()))
        })?;
        assert_eq!(item.and_then(|item| item.text()).as_deref(), Some(expected));
        selection = b"second\n".to_vec();
    }
    assert_eq!(calls, 2);
    Ok(())
}

#[test]
fn unavailable_wayland_selection_never_reads_the_xwayland_clipboard() -> Result<()> {
    let item = read_commands(false, true, true, |program, _, _, _| {
        assert_eq!(
            program, "wl-paste",
            "X11 may still contain the previous copy"
        );
        Ok(None)
    })?;
    assert!(item.is_none());
    Ok(())
}

#[test]
fn wayland_plain_text_fallback_precedes_any_image_and_preserves_empty_text() -> Result<()> {
    let mut calls = Vec::new();
    let item = read_commands(false, true, true, |program, args, limit, _| {
        assert_eq!(program, "wl-paste");
        assert_eq!(limit, TEXT_LIMIT);
        calls.push(args[2].to_owned());
        Ok((args[2] == "text/plain").then(Vec::new))
    })?;
    assert_eq!(calls, ["text/plain;charset=utf-8", "text/plain"]);
    assert!(matches!(item.as_ref().map(ClipboardItem::entries),
        Some([gpui::ClipboardEntry::String(entry)]) if entry.text().is_empty()));
    Ok(())
}

#[test]
fn helper_errors_do_not_fall_back_to_another_selection_or_format() {
    let result = read_commands(false, true, true, |program, _, _, _| {
        assert_eq!(program, "wl-paste");
        Err(Error::Process(io::Error::from(io::ErrorKind::NotFound)).into())
    });
    assert!(matches!(result,
        Err(crate::Error::Clipboard(Error::Process(error)))
            if error.kind() == io::ErrorKind::NotFound));
}

/// An X11 session can inherit `WAYLAND_DISPLAY` from the display manager. The
/// variable alone then names a compositor that is not running, so the session
/// must be decided by the socket itself. Wayland is Linux-only, so this runs
/// only there.
#[cfg(target_os = "linux")]
#[test]
fn wayland_session_requires_the_compositor_socket() -> Result<()> {
    use std::{ffi::OsStr, os::unix::net::UnixListener, path::PathBuf};

    // /tmp keeps the socket path well below SUN_LEN even when TMPDIR is deep.
    let root = if std::path::Path::new("/tmp").is_dir() {
        PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    // A private, exclusively created directory: a predictable name in shared
    // /tmp could be pre-created by another local user and redirect the writes.
    let dir = tempfile::Builder::new()
        .prefix("herdr-clipboard-")
        .tempdir_in(&root)
        .map_err(Error::Process)?;
    (|| {
        let name = OsStr::new("wayland-9");
        assert!(
            !wayland_socket(Some(name), Some(dir.path().as_os_str())),
            "a name without a socket is not a session"
        );
        let socket = dir.path().join(name);
        let listener = UnixListener::bind(&socket).map_err(Error::Process)?;
        assert!(
            wayland_socket(Some(name), Some(dir.path().as_os_str())),
            "a live compositor socket is a session"
        );
        assert!(
            wayland_socket(Some(socket.as_os_str()), None),
            "an absolute socket path needs no XDG_RUNTIME_DIR"
        );
        drop(listener);
        // The kernel releases a listening socket asynchronously, so the stale
        // socket can still answer for a few milliseconds. Wait for it to start
        // refusing instead of assuming a fixed delay is enough.
        let settled = Instant::now() + Duration::from_millis(500);
        while wayland_socket(Some(socket.as_os_str()), None) {
            assert!(
                Instant::now() < settled,
                "a socket left behind by a crashed compositor kept answering"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let regular = dir.path().join("regular");
        std::fs::write(&regular, b"").map_err(Error::Process)?;
        assert!(
            !wayland_socket(Some(regular.as_os_str()), None),
            "a regular file is not a compositor"
        );
        assert!(
            !wayland_socket(Some(name), None),
            "a relative name without XDG_RUNTIME_DIR cannot resolve"
        );
        assert!(
            !wayland_socket(None, Some(dir.path().as_os_str())),
            "no WAYLAND_DISPLAY is an X11 session"
        );
        Ok(())
    })()
}

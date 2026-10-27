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

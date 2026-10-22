mod cleanup;

use super::*;
use gpui::{ClipboardEntry, ExternalPaths, Image, ImageFormat};
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

fn round_trip(item: Option<&ClipboardItem>) -> crate::Result<Option<ClipboardItem>> {
    let mut bytes = Vec::new();
    wire::write(item, &mut bytes)?;
    wire::read(bytes)
}

#[test]
fn text_images_and_empty_clipboards_keep_their_contents() -> crate::Result<()> {
    assert!(round_trip(None)?.is_none());
    for text in ["", "语音输入\nsecond line\twith spaces", "text\0with NUL"] {
        let item = ClipboardItem::new_string(text.into());
        assert_eq!(
            round_trip(Some(&item))?
                .as_ref()
                .map(ClipboardItem::entries)
                .and_then(|entries| match entries {
                    [ClipboardEntry::String(value)] => Some(value.text().as_str()),
                    _ => None,
                }),
            Some(text)
        );
    }
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Gif,
        ImageFormat::Webp,
        ImageFormat::Bmp,
        ImageFormat::Tiff,
    ] {
        let image = Image::from_bytes(format, vec![0, 1, 128, 255]);
        let item = ClipboardItem::from(image.clone());
        let result = round_trip(Some(&item))?;
        assert!(matches!(result.as_ref().map(ClipboardItem::entries),
            Some([ClipboardEntry::Image(actual)]) if *actual == image));
    }
    Ok(())
}

#[test]
fn file_paths_preserve_order_non_utf8_and_the_drop_overflow_sentinel() -> crate::Result<()> {
    let mut paths: Vec<_> = (0..super::super::super::file_drop::MAX_PATHS)
        .map(|index| PathBuf::from(format!("/tmp/file {index}\n.png")))
        .collect();
    paths.push(PathBuf::from(OsString::from_vec(b"/tmp/\xff.png".to_vec())));
    let item = ClipboardItem {
        entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
            paths.clone().into(),
        ))],
    };
    let result = round_trip(Some(&item))?;
    assert!(matches!(result.as_ref().map(ClipboardItem::entries),
        Some([ClipboardEntry::ExternalPaths(actual)]) if actual.paths() == paths));
    Ok(())
}

#[test]
fn incomplete_unknown_and_oversized_responses_are_rejected() {
    for bytes in [
        vec![],
        vec![0, 1],
        vec![255],
        vec![2],
        b"\x08/tmp/no-terminator".to_vec(),
        b"\x08relative\0".to_vec(),
        b"\x08/tmp/a\0\0".to_vec(),
    ] {
        assert!(matches!(
            wire::read(bytes),
            Err(crate::Error::Clipboard(ClipboardError::Native(
                Error::Response
            )))
        ));
    }
    assert!(matches!(
        wire::read(vec![1, 255]),
        Err(crate::Error::Clipboard(ClipboardError::Encoding(_)))
    ));
    let mut bytes = vec![b'x'; TEXT_LIMIT + 2];
    bytes[0] = 1;
    assert!(matches!(
        wire::read(bytes),
        Err(crate::Error::Clipboard(ClipboardError::Size {
            limit: TEXT_LIMIT
        }))
    ));
    let mut paths = vec![8];
    for _ in 0..super::super::super::file_drop::MAX_PATHS + 2 {
        paths.extend_from_slice(b"/tmp/a\0");
    }
    assert!(matches!(
        wire::read(paths),
        Err(crate::Error::Clipboard(ClipboardError::Native(
            Error::Response
        )))
    ));
}

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

#[test]
fn successful_child_output_is_the_only_data_pasted() -> crate::Result<()> {
    let item = acquire(
        &mut shell(r"printf '\001voice input'"),
        Instant::now() + TIMEOUT,
        100,
    )?;
    assert_eq!(
        item.and_then(|item| item.text()).as_deref(),
        Some("voice input")
    );
    for script in [
        r"printf '\001partial'; exit 1",
        r"printf '\001partial'; kill -KILL $$",
    ] {
        assert!(matches!(
            acquire(&mut shell(script), Instant::now() + TIMEOUT, 100),
            Err(crate::Error::Clipboard(ClipboardError::Native(
                Error::Exited(_)
            )))
        ));
    }
    Ok(())
}

#[test]
fn native_failures_keep_changed_encoding_and_size_diagnostics() {
    for (code, expected) in [
        (CHANGED, 0),
        (INVALID_TEXT, 1),
        (TEXT_TOO_LARGE, 2),
        (IMAGE_TOO_LARGE, 3),
    ] {
        let result = acquire(
            &mut shell(&format!("exit {code}")),
            Instant::now() + TIMEOUT,
            100,
        );
        match (expected, result) {
            (0, Err(crate::Error::Clipboard(ClipboardError::Changed)))
            | (1, Err(crate::Error::Clipboard(ClipboardError::Native(Error::TextEncoding))))
            | (2, Err(crate::Error::Clipboard(ClipboardError::Size { limit: TEXT_LIMIT })))
            | (3, Err(crate::Error::Clipboard(ClipboardError::Size { limit: IMAGE_LIMIT }))) => {}
            (_, other) => panic!("unexpected clipboard result: {other:?}"),
        }
    }
}

#[test]
fn helper_arguments_cannot_fall_through_to_normal_app_startup() {
    assert!(run_helper(&["--help".into()]).is_none());
    for args in [
        vec![HELPER],
        vec![HELPER, "wrong"],
        vec![HELPER, "normal", "extra"],
    ] {
        assert_eq!(
            run_helper(&args.into_iter().map(OsString::from).collect::<Vec<_>>()),
            Some(ExitCode::from(2))
        );
    }
}

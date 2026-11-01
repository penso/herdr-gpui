#![allow(clippy::unwrap_used)]
use super::*;
use herdr_client::ConnectTarget;
use std::io;

mod en_xa;
mod paste_errors;

fn processing_errors() -> Vec<Error> {
    vec![
        Error::ImageInputTooLarge {
            limit: image_source::MAX_IMAGE_INPUT_BYTES,
        },
        Error::ImageTooLarge {
            limit: herdr_client::protocol::MAX_CLIPBOARD_IMAGE_PAYLOAD,
        },
        Error::ImageDecodeLimit,
        Error::ImageAnimationResize,
        Error::ImageDecode(image::ImageError::IoError(io::Error::other(
            "/private/image.png secret decoder diagnostic",
        ))),
        Error::ImageEncode(image::ImageError::IoError(io::Error::other(
            "/private/image.png secret encoder diagnostic",
        ))),
        Error::ImageReadTimeout,
        Error::ImageFormat,
        crate::window::clipboard::Error::Size {
            limit: image_source::MAX_IMAGE_INPUT_BYTES,
        }
        .into(),
        crate::window::clipboard::Error::Changed.into(),
        crate::window::clipboard::Error::Timeout.into(),
        crate::window::clipboard::Error::Process(io::Error::other("secret clipboard diagnostic"))
            .into(),
    ]
}

#[test]
fn file_availability_errors_alone_preserve_original_paste() {
    let text = "\x1b[200~'/private/original image.png'\x1b[201~\r\n";
    for error in [
        Error::ImageFile {
            operation: "open",
            source: io::ErrorKind::NotFound.into(),
        },
        Error::ImageFile {
            operation: "read",
            source: io::ErrorKind::PermissionDenied.into(),
        },
        Error::ImageFileType,
        Error::ImageSize,
    ] {
        assert!(matches!(
            Prepared::image(Err(error), Some(text.into())),
            Ok(Prepared::Input(ClientPaneInputEvent::Paste(actual))) if actual == text
        ));
    }
    assert!(matches!(
        Prepared::image(Err(Error::ImageSize), None),
        Err(Error::ImageSize)
    ));
    for error in processing_errors() {
        let kind = std::mem::discriminant(&error);
        assert!(matches!(
            Prepared::image(Err(error), Some(text.into())),
            Err(actual) if std::mem::discriminant(&actual) == kind
        ));
    }
}

#[test]
fn prepared_image_preserves_owned_bytes_and_resize_flag() {
    for resized in [false, true] {
        let bytes = vec![1, 2, 3];
        let pointer = bytes.as_ptr();
        let image = PreparedImage {
            extension: "png",
            bytes,
            resized,
        };
        let prepared = Prepared::image(Ok(image), Some("unused fallback".into())).unwrap();
        let Prepared::Image(image) = prepared else {
            panic!("image replaced by fallback");
        };
        assert_eq!(image.bytes.as_ptr(), pointer);
        assert_eq!(image.bytes, [1, 2, 3]);
        assert_eq!(image.extension, "png");
        assert_eq!(image.resized, resized);
    }
}

#[gpui::test]
fn all_processing_errors_show_redacted_local_feedback(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let mut errors = processing_errors();
    errors.push(Error::ImageFile {
        operation: "read",
        source: io::Error::other("/private/image.png secret file diagnostic"),
    });
    errors.extend([Error::ImageSize, Error::ImageFileType]);
    for error in errors {
        view.update(cx, |view, cx| {
            view.config.notifications.enabled = false;
            view.config.notifications.delay_seconds = 3600;
            view.endpoints[0].toasts.entries.clear();
            view.report_preparation_error(PreparationKind::Image, &error, cx);
            let (_, notice) = view.endpoints[0].toasts.entries.back().unwrap();
            assert_eq!(notice.title, "Image discarded");
            assert!(notice.visible);
            assert_eq!(
                notice.body.as_deref(),
                Some(format!("{error} Nothing was uploaded or pasted.").as_str())
            );
            for text in [
                notice.body.as_ref().unwrap(),
                view.local_error.as_ref().unwrap(),
            ] {
                assert!(!text.contains("/private"));
                assert!(!text.contains("secret"));
            }
        });
    }
}

#[gpui::test]
fn shared_transfer_notice_uses_selected_endpoint_and_sanitizes_text(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "other".into(),
            "Other".into(),
            ConnectTarget::Socket("/unused-transfer-test.sock".into()),
            false,
        ));
        view.selected_endpoint = 1;
        view.local_transfer_notice("Copy failed", "Cannot copy\x1b\ntext".into(), cx);
        assert!(view.endpoints[0].toasts.entries.is_empty());
        let (_, notice) = view.endpoints[1].toasts.entries.back().unwrap();
        assert_eq!(notice.title, "Copy failed");
        assert_eq!(notice.body.as_deref(), Some("Cannot copytext"));
        assert!(notice.visible);
        assert!(view.local_error.is_none());
    });
}

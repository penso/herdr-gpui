use super::*;
use crate::web::uploads::{Error, MAX_UPLOAD, sniff};
use herdr_chat_protocol::{Attachment, ImageType};

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];

#[test]
fn pictures_are_known_by_their_bytes_not_their_name() {
    assert_eq!(sniff(PNG), Some(ImageType::Png));
    assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some(ImageType::Jpeg));
    assert_eq!(sniff(b"GIF89a.."), Some(ImageType::Gif));
    assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(ImageType::Webp));
    assert_eq!(sniff(b"#!/bin/sh\nrm -rf /"), None);
    assert_eq!(sniff(b""), None);
}

#[test]
fn an_upload_is_attached_by_id_and_stored_privately() {
    let dir = tempfile::tempdir().unwrap();
    let uploads = Uploads::new(dir.path());
    let id = uploads.store("abc123".into(), PNG, 1_000).unwrap();
    let resolved = uploads.resolve(&[Attachment { path: id }], 1_000).unwrap();
    let path = std::path::Path::new(&resolved[0].path);
    assert!(path.starts_with(dir.path().join("uploads")));
    assert_eq!(std::fs::read(path).unwrap(), PNG);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn a_phone_cannot_attach_a_path_or_a_non_picture() {
    let dir = tempfile::tempdir().unwrap();
    let uploads = Uploads::new(dir.path());
    assert!(matches!(
        uploads.resolve(
            &[Attachment {
                path: "/etc/passwd".into()
            }],
            0
        ),
        Err(Error::UnknownUpload)
    ));
    assert!(matches!(
        uploads.store("x".into(), b"not a picture", 0),
        Err(Error::NotAnImage)
    ));
    let mut huge = PNG.to_vec();
    huge.resize(MAX_UPLOAD + 1, 0);
    assert!(matches!(
        uploads.store("y".into(), &huge, 0),
        Err(Error::TooLarge)
    ));
}

#[test]
fn uploads_expire_after_a_day() {
    let dir = tempfile::tempdir().unwrap();
    let uploads = Uploads::new(dir.path());
    let day = 24 * 60 * 60 * 1000;
    let id = uploads.store("old".into(), PNG, 0).unwrap();
    let attachment = [Attachment { path: id }];
    let path = uploads.resolve(&attachment, day - 1).unwrap()[0]
        .path
        .clone();
    // Past its lifetime it cannot be attached, even before a sweep.
    assert!(matches!(
        uploads.resolve(&attachment, day),
        Err(Error::UnknownUpload)
    ));
    uploads.prune(day);
    assert!(!std::path::Path::new(&path).exists());
}

#[test]
fn pictures_from_an_earlier_run_are_deleted_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let earlier = Uploads::new(dir.path());
    let id = earlier.store("left".into(), PNG, 0).unwrap();
    let path = earlier.resolve(&[Attachment { path: id }], 0).unwrap()[0]
        .path
        .clone();
    let restarted = Uploads::new(dir.path());
    restarted.remove_leftovers();
    assert!(!std::path::Path::new(&path).exists());
    // A missing folder is fine.
    Uploads::new(&dir.path().join("none")).remove_leftovers();
}

#[tokio::test]
async fn uploading_to_an_unknown_chat_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let request = HttpRequest::post("/api/chat/p9/upload")
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(PNG))
        .unwrap();
    // No session: refused before anything is stored.
    assert_eq!(send(&state, request).await.0, StatusCode::UNAUTHORIZED);
}

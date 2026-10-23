//! Real AppKit pasteboards, each privately named so the user's clipboard is untouched.
#![allow(clippy::unwrap_used)]
use super::*;
use gpui::ClipboardEntry;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};
use std::path::{Path, PathBuf};

/// One pasteboard item per entry, each holding `(type, string or bytes)` pairs.
fn pasteboard(items: &[&[(&str, Vec<u8>)]]) -> Retained<NSPasteboard> {
    let pasteboard = NSPasteboard::pasteboardWithUniqueName();
    pasteboard.clearContents();
    let items: Vec<_> = items
        .iter()
        .map(|types| {
            let item = NSPasteboardItem::new();
            for (kind, bytes) in *types {
                let kind = NSString::from_str(kind);
                assert!(item.setData_forType(&NSData::with_bytes(bytes), &kind));
            }
            ProtocolObject::<dyn NSPasteboardWriting>::from_retained(item)
        })
        .collect();
    assert!(pasteboard.writeObjects(&NSArray::from_retained_slice(&items)));
    pasteboard
}

fn file_url(path: &Path) -> Vec<u8> {
    NSURL::from_file_path(path)
        .unwrap()
        .absoluteString()
        .unwrap()
        .to_string()
        .into_bytes()
}

fn paths(item: Option<ClipboardItem>) -> Vec<PathBuf> {
    match item.as_ref().map(ClipboardItem::entries) {
        Some([ClipboardEntry::ExternalPaths(paths)]) => paths.paths().to_vec(),
        other => panic!("expected only external paths, got {other:?}"),
    }
}

/// What Finder publishes for Cmd-C on a file: its URL, its name as text, and
/// preview images. A paste reads the path; Ctrl-V's image-only read is unchanged.
#[test]
fn a_finder_file_copy_reads_its_full_path_not_its_name() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("Screenshot 2026-10-14 at 11.01.09 AM.png");
    std::fs::write(&path, [1, 2, 3]).unwrap();
    let name = "Screenshot 2026-10-14 at 11.01.09 AM.png";
    objc2::rc::autoreleasepool(|_| {
        let pasteboard = pasteboard(&[&[
            ("public.file-url", file_url(&path)),
            ("public.utf8-plain-text", name.as_bytes().to_vec()),
            ("public.png", vec![9, 8, 7]),
        ]]);
        assert_eq!(paths(read_pasteboard(&pasteboard, false).unwrap()), [path]);
        let image = read_pasteboard(&pasteboard, true).unwrap();
        assert!(matches!(image.as_ref().map(ClipboardItem::entries),
            Some([ClipboardEntry::Image(image)]) if image.bytes == [9, 8, 7]));
        pasteboard.clearContents();
    });
}

#[test]
fn several_files_keep_their_order_and_reference_urls_resolve() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let first = root.join("it's a file.txt");
    let second = root.join("second.png");
    for path in [&first, &second] {
        std::fs::write(path, [0]).unwrap();
    }
    objc2::rc::autoreleasepool(|_| {
        // Finder can publish `file:///.file/id=...` reference URLs.
        let reference = NSURL::from_file_path(&second)
            .unwrap()
            .fileReferenceURL()
            .unwrap()
            .absoluteString()
            .unwrap()
            .to_string();
        assert!(reference.starts_with("file:///.file/id="), "{reference}");
        let pasteboard = pasteboard(&[
            &[("public.file-url", file_url(&first))],
            &[("public.file-url", reference.into_bytes())],
        ]);
        assert_eq!(
            paths(read_pasteboard(&pasteboard, false).unwrap()),
            [first, second]
        );
        pasteboard.clearContents();
    });
}

#[test]
fn text_copies_without_file_urls_still_paste_their_text() {
    objc2::rc::autoreleasepool(|_| {
        let pasteboard = pasteboard(&[&[
            ("public.utf8-plain-text", b"plain text".to_vec()),
            ("public.url", b"https://example.com/".to_vec()),
        ]]);
        let item = read_pasteboard(&pasteboard, false).unwrap();
        assert!(matches!(item.as_ref().map(ClipboardItem::entries),
            Some([ClipboardEntry::String(text)]) if text.text() == "plain text"));
        pasteboard.clearContents();
    });
}

/// A URL that no longer resolves must not use up the bound and hide a later
/// file: a copy one path over the drop's limit still fails that limit.
#[test]
fn unresolved_urls_never_hide_files_past_the_bound() {
    use super::super::file_drop::{MAX_PATHS, quote_paths};

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let files: Vec<_> = (0..=MAX_PATHS)
        .map(|n| root.join(format!("{n}.txt")))
        .collect();
    for path in &files {
        std::fs::write(path, [0]).unwrap();
    }
    objc2::rc::autoreleasepool(|_| {
        let gone = root.join("deleted.txt");
        std::fs::write(&gone, [0]).unwrap();
        let reference = NSURL::from_file_path(&gone)
            .unwrap()
            .fileReferenceURL()
            .unwrap()
            .absoluteString()
            .unwrap()
            .to_string();
        std::fs::remove_file(&gone).unwrap();
        let types: Vec<Vec<(&str, Vec<u8>)>> =
            std::iter::once(vec![("public.file-url", reference.into_bytes())])
                .chain(
                    files
                        .iter()
                        .map(|path| vec![("public.file-url", file_url(path))]),
                )
                .collect();
        let items: Vec<&[(&str, Vec<u8>)]> = types.iter().map(Vec::as_slice).collect();
        let pasteboard = pasteboard(&items);
        let read = paths(read_pasteboard(&pasteboard, false).unwrap());
        assert_eq!(read, files);
        assert!(matches!(
            quote_paths(&read),
            Err(Error::FileDrop(super::super::file_drop::Error::Size))
        ));
        pasteboard.clearContents();
    });
}

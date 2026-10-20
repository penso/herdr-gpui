#![allow(clippy::unwrap_used)]

use super::*;
use std::io::Write;

/// A gzipped tar of one regular file whose path the tar writer would itself
/// refuse, so the name goes straight into the header.
fn raw_tar(path: &str, bytes: &[u8]) -> Vec<u8> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    let name = header.as_mut_bytes();
    name[..path.len()].copy_from_slice(path.as_bytes());
    header.set_cksum();
    let mut archive = header.as_bytes().to_vec();
    archive.extend_from_slice(bytes);
    archive.resize(archive.len() + (512 - bytes.len() % 512) % 512, 0);
    archive.extend_from_slice(&[0; 1024]);
    gzip(&archive)
}

/// A gzipped tar holding `entries` as regular files.
fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, *bytes).unwrap();
    }
    gzip(&builder.into_inner().unwrap())
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// A zip holding `entries`; a path ending in `/` is a directory.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (path, bytes) in entries {
        if path.ends_with('/') {
            writer.add_directory(*path, options).unwrap();
        } else {
            writer.start_file(*path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

/// A gzipped tar whose one link entry points at `target`.
fn tar_gz_with_link(path: &str, target: &str) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_link_name(target).unwrap();
    header.set_cksum();
    builder.append_data(&mut header, path, io::empty()).unwrap();
    gzip(&builder.into_inner().unwrap())
}

/// Runs an extraction of `bytes` in a temporary folder, returning the folder
/// (holding `archive` and `staging`) and the result.
fn run(format: ArchiveFormat, bytes: &[u8], top: &str) -> (tempfile::TempDir, Result<Extracted>) {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("archive");
    let staging = root.path().join("staging");
    std::fs::write(&archive, bytes).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let cancelled = AtomicBool::new(false);
    let result = extract(&archive, &staging, top, format, &cancelled);
    (root, result)
}

fn staging(root: &tempfile::TempDir) -> PathBuf {
    root.path().join("staging")
}

#[test]
fn a_tar_tree_round_trips() {
    let bytes = tar_gz(&[("src/notes.md", b"notes"), ("src/sub/deep.txt", b"deep")]);
    let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    let extracted = result.unwrap();
    assert_eq!(extracted.links_skipped, 0);
    assert_eq!(
        std::fs::read(staging(&root).join("notes.md")).unwrap(),
        b"notes"
    );
    assert_eq!(
        std::fs::read(staging(&root).join("sub/deep.txt")).unwrap(),
        b"deep"
    );
}

#[test]
fn a_zip_tree_round_trips() {
    let bytes = zip(&[
        ("src/", b""),
        ("src/sub/", b""),
        ("src/notes.md", b"notes"),
        ("src/sub/deep.txt", b"deep"),
    ]);
    let (root, result) = run(ArchiveFormat::Zip, &bytes, "src");
    let extracted = result.unwrap();
    assert_eq!(extracted.links_skipped, 0);
    assert!(staging(&root).join("sub").is_dir());
    assert_eq!(
        std::fs::read(staging(&root).join("sub/deep.txt")).unwrap(),
        b"deep"
    );
}

#[test]
fn a_path_that_leaves_the_top_is_refused() {
    for (path, expected_top) in [
        ("../x", true),
        ("/etc/passwd", true),
        ("C:/Windows/x", true),
        ("src/../x", false),
        ("src/./x", false),
        ("src//x", false),
    ] {
        let bytes = raw_tar(path, b"x");
        let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
        let error = result.unwrap_err();
        if expected_top {
            assert!(matches!(error, Error::Top { .. }), "{path}: {error}");
        } else {
            assert!(matches!(error, Error::Path { .. }), "{path}: {error}");
        }
        assert_eq!(
            std::fs::read_dir(staging(&root)).unwrap().count(),
            0,
            "{path}: nothing is written"
        );
    }
}

#[test]
fn a_zip_path_that_leaves_the_top_is_refused() {
    for path in ["../x", "/etc/passwd", "C:/Windows/x", "src/../x"] {
        let bytes = zip(&[(path, b"x")]);
        let (root, result) = run(ArchiveFormat::Zip, &bytes, "src");
        assert!(result.is_err(), "{path}");
        assert_eq!(std::fs::read_dir(staging(&root)).unwrap().count(), 0);
    }
}

#[test]
fn an_archive_of_another_folder_is_refused() {
    let bytes = tar_gz(&[("other/x", b"x")]);
    let (_, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    assert!(matches!(result.unwrap_err(), Error::Top { .. }));
}

#[test]
fn a_tar_link_is_skipped_and_counted() {
    let bytes = tar_gz_with_link("src/link", "../../outside");
    let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    let extracted = result.unwrap();
    assert_eq!(extracted.links_skipped, 1);
    assert!(!staging(&root).join("link").exists(), "no link is created");
}

#[test]
fn a_hard_link_is_skipped_and_counted() {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Link);
    header.set_size(0);
    header.set_mode(0o644);
    header.set_link_name("src/notes.md").unwrap();
    header.set_cksum();
    builder
        .append_data(&mut header, "src/hard", io::empty())
        .unwrap();
    let bytes = gzip(&builder.into_inner().unwrap());
    let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    let extracted = result.unwrap();
    assert_eq!(extracted.links_skipped, 1);
    assert!(!staging(&root).join("hard").exists());
}

#[test]
fn more_than_the_entry_cap_is_refused() {
    let entries: Vec<(String, &[u8])> = (0..=MAX_ENTRIES)
        .map(|index| (format!("src/file{index}"), b"".as_slice()))
        .collect();
    let entries: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(path, bytes)| (path.as_str(), *bytes))
        .collect();
    let (_, result) = run(ArchiveFormat::TarGz, &tar_gz(&entries), "src");
    assert!(
        matches!(result.unwrap_err(), Error::TooManyEntries { .. }),
        "one entry past the cap is refused"
    );
}

#[test]
fn an_archive_that_expands_past_the_cap_is_refused() {
    let mut header = tar::Header::new_gnu();
    header.set_size(MAX_EXPANDED + 1);
    header.set_mode(0o644);
    header.set_path("src/big").unwrap();
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(&[0; 1024]);
    let (root, result) = run(ArchiveFormat::TarGz, &gzip(&bytes), "src");
    assert!(matches!(
        result.unwrap_err(),
        Error::ExpandedTooLarge { .. }
    ));
    assert_eq!(std::fs::read_dir(staging(&root)).unwrap().count(), 0);
}

#[test]
fn a_name_windows_cannot_store_is_sanitized() {
    let bytes = tar_gz(&[("src/a:b.txt", b"x")]);
    let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    result.unwrap();
    let name = if cfg!(windows) { "a_b.txt" } else { "a:b.txt" };
    assert_eq!(
        std::fs::read(staging(&root).join(name)).unwrap(),
        b"x",
        "the entry is stored under a storable name"
    );
}

#[test]
fn two_entries_with_one_path_are_refused() {
    let bytes = tar_gz(&[("src/a.txt", b"first"), ("src/a.txt", b"second")]);
    let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    assert!(matches!(result.unwrap_err(), Error::Collision { .. }));
    assert_eq!(
        std::fs::read(staging(&root).join("a.txt")).unwrap(),
        b"first",
        "the second entry never replaces the first"
    );
}

#[cfg(windows)]
#[test]
fn two_names_that_sanitize_alike_are_refused() {
    let bytes = tar_gz(&[("src/a:b", b"first"), ("src/a?b", b"second")]);
    let (root, result) = run(ArchiveFormat::TarGz, &bytes, "src");
    assert!(matches!(result.unwrap_err(), Error::Collision { .. }));
    assert_eq!(
        std::fs::read(staging(&root).join("a_b")).unwrap(),
        b"first",
        "the second entry never replaces the first"
    );
}

#[test]
fn a_cancelled_extraction_stops_before_writing() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("archive");
    let staging = root.path().join("staging");
    std::fs::write(&archive, tar_gz(&[("src/a.txt", b"x")])).unwrap();
    std::fs::create_dir(&staging).unwrap();
    let cancelled = AtomicBool::new(true);
    let result = extract(&archive, &staging, "src", ArchiveFormat::TarGz, &cancelled);
    assert!(matches!(result.unwrap_err(), Error::Cancelled));
    assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
}

#[test]
fn a_truncated_archive_is_an_error() {
    let bytes = tar_gz(&[("src/a.txt", b"some bytes")]);
    let truncated = &bytes[..bytes.len() - 8];
    let (_, result) = run(ArchiveFormat::TarGz, truncated, "src");
    assert!(matches!(result.unwrap_err(), Error::Read(_)));
}

#[test]
fn a_zip_directory_outside_the_top_is_refused() {
    let bytes = zip(&[("other/", b""), ("src/a.txt", b"a")]);
    let (root, result) = run(ArchiveFormat::Zip, &bytes, "src");
    assert!(matches!(result.unwrap_err(), Error::Top { .. }));
    assert_eq!(std::fs::read_dir(staging(&root)).unwrap().count(), 0);
}

#[test]
fn a_zip_crc_error_surfaces() {
    let mut bytes = {
        let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("src/a.txt", options).unwrap();
        writer.write_all(b"aaaaaaaaaaaaaaaa").unwrap();
        writer.finish().unwrap().into_inner()
    };
    let at = bytes
        .windows(16)
        .position(|window| window == b"aaaaaaaaaaaaaaaa")
        .unwrap();
    bytes[at] = b'b';
    let (_, result) = run(ArchiveFormat::Zip, &bytes, "src");
    assert!(matches!(result.unwrap_err(), Error::Write { .. }));
}

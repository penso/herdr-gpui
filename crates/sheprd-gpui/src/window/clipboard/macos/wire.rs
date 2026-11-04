//! Private, bounded stdout format between copies of the same executable. A
//! one-byte tag is followed by raw text/image bytes or NUL-delimited paths.
//! No clipboard contents are written to disk or included in diagnostics.

use super::{Error, IMAGE_LIMIT, TEXT_LIMIT};
use crate::window::clipboard;
use gpui::{ClipboardEntry, ClipboardItem, ExternalPaths, Image, ImageFormat};
use std::{
    ffi::OsString,
    io::Write,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
};

const EMPTY: u8 = 0;
const TEXT: u8 = 1;
const FILES: u8 = 8;
const FORMATS: &[(u8, ImageFormat)] = &[
    (2, ImageFormat::Png),
    (3, ImageFormat::Jpeg),
    (4, ImageFormat::Gif),
    (5, ImageFormat::Webp),
    (6, ImageFormat::Bmp),
    (7, ImageFormat::Tiff),
];

pub(super) fn write(item: Option<&ClipboardItem>, output: &mut impl Write) -> crate::Result<()> {
    let (tag, bytes) = match item.map(ClipboardItem::entries) {
        None => (EMPTY, &[][..]),
        Some([ClipboardEntry::String(text)]) => (TEXT, text.text().as_bytes()),
        Some([ClipboardEntry::Image(image)]) => (
            FORMATS
                .iter()
                .find(|(_, format)| *format == image.format)
                .map(|(tag, _)| *tag)
                .ok_or(Error::Response)?,
            image.bytes.as_slice(),
        ),
        Some([ClipboardEntry::ExternalPaths(paths)]) => {
            let paths = paths.paths();
            if paths.is_empty() || paths.len() > super::super::super::file_drop::MAX_PATHS + 1 {
                return Err(Error::Response.into());
            }
            let size = paths.iter().try_fold(0usize, |size, path| {
                size.checked_add(path.as_os_str().len())?.checked_add(1)
            });
            if size.is_none_or(|size| size > TEXT_LIMIT) {
                return Err(clipboard::Error::Size { limit: TEXT_LIMIT }.into());
            }
            output.write_all(&[FILES]).map_err(Error::Process)?;
            for path in paths {
                let bytes = path.as_os_str().as_bytes();
                if !path.is_absolute() || bytes.contains(&0) {
                    return Err(Error::Response.into());
                }
                output.write_all(bytes).map_err(Error::Process)?;
                output.write_all(&[0]).map_err(Error::Process)?;
            }
            return Ok(());
        }
        _ => return Err(Error::Response.into()),
    };
    output.write_all(&[tag]).map_err(Error::Process)?;
    output.write_all(bytes).map_err(Error::Process)?;
    Ok(())
}

pub(super) fn read(mut bytes: Vec<u8>) -> crate::Result<Option<ClipboardItem>> {
    if bytes.is_empty() {
        return Err(Error::Response.into());
    }
    let tag = bytes.remove(0);
    match tag {
        EMPTY if bytes.is_empty() => Ok(None),
        TEXT => {
            if bytes.len() > TEXT_LIMIT {
                return Err(clipboard::Error::Size { limit: TEXT_LIMIT }.into());
            }
            let text = String::from_utf8(bytes)
                .map_err(|error| clipboard::Error::Encoding(error.utf8_error()))?;
            Ok(Some(ClipboardItem::new_string(text)))
        }
        FILES => {
            if bytes.len() > TEXT_LIMIT {
                return Err(clipboard::Error::Size { limit: TEXT_LIMIT }.into());
            }
            let Some(bytes) = bytes.strip_suffix(&[0]) else {
                return Err(Error::Response.into());
            };
            let mut paths = Vec::new();
            for bytes in bytes.split(|byte| *byte == 0) {
                let path = PathBuf::from(OsString::from_vec(bytes.to_vec()));
                if !path.is_absolute() || paths.len() > super::super::super::file_drop::MAX_PATHS {
                    return Err(Error::Response.into());
                }
                paths.push(path);
            }
            Ok(Some(ClipboardItem {
                entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(paths.into()))],
            }))
        }
        _ => {
            let format = FORMATS
                .iter()
                .find(|(candidate, _)| *candidate == tag)
                .map(|(_, format)| *format)
                .ok_or(Error::Response)?;
            if bytes.is_empty() {
                return Err(Error::Response.into());
            }
            if bytes.len() > IMAGE_LIMIT {
                return Err(clipboard::Error::Size { limit: IMAGE_LIMIT }.into());
            }
            Ok(Some(Image::from_bytes(format, bytes).into()))
        }
    }
}

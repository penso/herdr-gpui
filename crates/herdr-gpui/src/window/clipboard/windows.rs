//! The Windows clipboard, read on a thread of its own. An owner that renders
//! its data on request answers inside `GetClipboardData`, and one that never
//! answers would stall whoever asked, so the caller waits for the reader only
//! until a deadline, and only one reader may be outstanding.

use super::*;
use clipboard_win::{Clipboard, formats, raw};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// The whole acquisition, as on Linux: opening the clipboard, then each format.
const DEADLINE: Duration = Duration::from_secs(3);

/// Set while a reader thread runs, including one its caller stopped waiting for.
static READING: AtomicBool = AtomicBool::new(false);

pub(super) fn read(image_only: bool) -> Result<Option<ClipboardItem>> {
    bounded(&READING, DEADLINE, move |deadline| {
        read_now(image_only, deadline)
    })
}

/// Run `read` on its own thread and wait for it until `timeout` passes. While
/// an earlier reader is still running, refuse rather than start another, so a
/// stuck clipboard owner holds at most one thread.
fn bounded<T: Send + 'static>(
    busy: &'static AtomicBool,
    timeout: Duration,
    read: impl FnOnce(Instant) -> Result<T> + Send + 'static,
) -> Result<T> {
    if busy.swap(true, Ordering::AcqRel) {
        return Err(Error::Clipboard(ClipboardError::Timeout));
    }
    let deadline = Instant::now() + timeout;
    let (sender, receiver) = mpsc::sync_channel(1);
    let spawned = thread::Builder::new()
        .name("herdr-clipboard".into())
        .spawn(move || {
            let result = read(deadline);
            busy.store(false, Ordering::Release);
            // The caller may have stopped waiting; the result is then dropped.
            let _ = sender.send(result);
        });
    if let Err(error) = spawned {
        busy.store(false, Ordering::Release);
        return Err(Error::Clipboard(ClipboardError::Process(error)));
    }
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| Error::Clipboard(ClipboardError::Timeout))?
}

fn read_now(image_only: bool, deadline: Instant) -> Result<Option<ClipboardItem>> {
    use gpui::{ClipboardEntry, ExternalPaths};

    let revision = raw::seq_num();
    let clipboard = open(deadline)?;
    let files = if image_only { None } else { files()? };
    let result = match files {
        Some(paths) => Ok(Some(ClipboardItem {
            entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(paths.into()))],
        })),
        None => read_with(image_only, |format, limit| {
            if Instant::now() >= deadline {
                return Err(Error::Clipboard(ClipboardError::Timeout));
            }
            acquire(format, limit)
        }),
    };
    drop(clipboard);
    if raw::seq_num() != revision {
        return Err(Error::Clipboard(ClipboardError::Changed));
    }
    result
}

/// Another program may hold the clipboard open for a moment; retry until the
/// deadline rather than fail the paste at once.
fn open(deadline: Instant) -> Result<Clipboard> {
    loop {
        match Clipboard::new() {
            Ok(clipboard) => return Ok(clipboard),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(_) => return Err(Error::Clipboard(ClipboardError::Timeout)),
        }
    }
}

fn system(error: clipboard_win::ErrorCode) -> Error {
    Error::Clipboard(ClipboardError::Process(std::io::Error::from_raw_os_error(
        error.raw_code(),
    )))
}

/// The paths of a file copy, one more than a drop accepts at most, so an
/// oversized copy still fails the drop's own limit.
fn files() -> Result<Option<Vec<PathBuf>>> {
    let Some(size) = raw::size(formats::CF_HDROP) else {
        return Ok(None);
    };
    if size.get() > TEXT_LIMIT {
        return Err(Error::Clipboard(ClipboardError::Size { limit: TEXT_LIMIT }));
    }
    let mut paths = Vec::new();
    raw::get_file_list_path(&mut paths).map_err(system)?;
    paths.truncate(super::super::file_drop::MAX_PATHS + 1);
    Ok((!paths.is_empty()).then_some(paths))
}

/// The names other Windows programs publish each image format under. WebP has
/// no common one, so it is never found here.
fn acquire(format: Option<ImageFormat>, limit: usize) -> Result<Option<Vec<u8>>> {
    let name = match format {
        None => return text(limit),
        Some(ImageFormat::Bmp) => return dib(limit),
        Some(ImageFormat::Tiff) => return bytes(formats::CF_TIFF, limit),
        Some(ImageFormat::Png) => "PNG",
        Some(ImageFormat::Jpeg) => "JFIF",
        Some(ImageFormat::Gif) => "GIF",
        Some(ImageFormat::Webp) => return Ok(None),
        Some(ImageFormat::Svg | ImageFormat::Ico | ImageFormat::Pnm) => {
            return Err(Error::ImageFormat);
        }
    };
    match raw::register_format(name) {
        Some(format) => bytes(format.get(), limit),
        None => Ok(None),
    }
}

/// One format's data, sized before any of it is copied.
fn bytes(format: u32, limit: usize) -> Result<Option<Vec<u8>>> {
    let Some(size) = raw::size(format) else {
        return Ok(None);
    };
    if size.get() > limit {
        return Err(Error::Clipboard(ClipboardError::Size { limit }));
    }
    let mut bytes = Vec::new();
    raw::get_vec(format, &mut bytes).map_err(system)?;
    Ok(Some(bytes))
}

/// Text as UTF-8. Each UTF-16 unit is at least one UTF-8 byte, so text whose
/// units, less the terminator, exceed the limit is refused before conversion.
fn text(limit: usize) -> Result<Option<Vec<u8>>> {
    let Some(size) = raw::size(formats::CF_UNICODETEXT) else {
        return Ok(None);
    };
    if size.get() / 2 > limit + 1 {
        return Err(Error::Clipboard(ClipboardError::Size { limit }));
    }
    let mut bytes = Vec::new();
    raw::get_string(&mut bytes).map_err(system)?;
    Ok(Some(bytes))
}

/// A device-independent bitmap, which nearly every Windows program offers, as
/// the BMP file it lacks only the header of.
fn dib(limit: usize) -> Result<Option<Vec<u8>>> {
    let Some(dib) = bytes(formats::CF_DIB, limit.saturating_sub(BMP_HEADER))? else {
        return Ok(None);
    };
    Ok(bmp_from_dib(&dib))
}

const BMP_HEADER: usize = 14;

/// Prepend the `BITMAPFILEHEADER` a DIB omits, or `None` when its info header
/// is too short or describes a pixel offset past the data.
fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let field = |at: usize, len: usize| dib.get(at..at + len);
    let u32_at = |at| Some(u32::from_le_bytes(field(at, 4)?.try_into().ok()?));
    let header = u32_at(0)?;
    if header < 40 || dib.len() < 40 {
        return None;
    }
    let bits = u16::from_le_bytes(field(14, 2)?.try_into().ok()?);
    let compression = u32_at(16)?;
    let palette = if bits <= 8 {
        let used = u32_at(32)?;
        u64::from(if used == 0 { 1 << bits } else { used }) * 4
    } else if compression == 3 && header == 40 {
        // BI_BITFIELDS: three masks follow a plain info header.
        12
    } else {
        0
    };
    let offset = BMP_HEADER as u64 + u64::from(header) + palette;
    let size = (BMP_HEADER + dib.len()) as u64;
    if offset > size {
        return None;
    }
    let mut bmp = Vec::with_capacity(BMP_HEADER + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&u32::try_from(size).ok()?.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&u32::try_from(offset).ok()?.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

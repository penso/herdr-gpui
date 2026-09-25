//! Disk-free source recognition; opaque image bytes are prepared off the UI thread.

use crate::{Error, Result};
use gpui_kit::{Image, ImageFormat};
use herdr_client::protocol::MAX_CLIPBOARD_IMAGE_PAYLOAD;
use image::{
    DynamicImage, ImageDecoder, ImageEncoder, ImageReader, Limits,
    codecs::{jpeg::JpegEncoder, png::PngDecoder, png::PngEncoder},
    metadata::Orientation,
};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::OpenOptions,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_READ_DURATION: Duration = Duration::from_secs(3);
/// Source acquisition cap, shared with native clipboard readers. Only encoded
/// inputs up to 128 MiB may enter the background resize fallback.
pub(super) const MAX_IMAGE_INPUT_BYTES: usize = 128 * 1024 * 1024;
const MAX_DECODE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

pub(super) struct PreparedImage {
    pub extension: &'static str,
    pub bytes: Vec<u8>,
    /// True when recompressed or downscaled, rather than passed through unchanged.
    pub resized: bool,
}

impl std::fmt::Debug for PreparedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedImage")
            .field("extension", &self.extension)
            .field("byte_count", &self.bytes.len())
            .field("resized", &self.resized)
            .finish()
    }
}

// Deliberately no Debug: neither local paths nor clipboard bytes belong in logs.
pub(super) enum Source {
    File {
        path: PathBuf,
        extension: &'static str,
    },
    Clipboard(Image),
}

pub(super) fn from_path(path: &Path) -> Option<Source> {
    if path.as_os_str().len() > MAX_SOURCE_BYTES || !path.is_absolute() {
        return None;
    }
    let extension = path.extension()?.to_str()?;
    let extension = ["png", "jpg", "gif", "webp", "bmp"]
        .into_iter()
        .find(|candidate| extension.eq_ignore_ascii_case(candidate))
        .or_else(|| extension.eq_ignore_ascii_case("jpeg").then_some("jpg"))?;
    Some(Source::File {
        path: path.to_owned(),
        extension,
    })
}

/// Match upstream's Unix terminal-drop escaping, even inside matching quotes.
/// Remote GUI image bridging is Unix-only; this is not Windows shell parsing.
pub(super) fn from_paste(text: &str) -> Option<Source> {
    if text.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let text = text
        .strip_prefix("\x1b[200~")
        .and_then(|text| text.strip_suffix("\x1b[201~"))
        .unwrap_or(text)
        .trim_end_matches(['\r', '\n']);
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    let text = if text.len() >= 2
        && matches!(
            (text.as_bytes().first(), text.as_bytes().last()),
            (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"'))
        ) {
        &text[1..text.len() - 1]
    } else {
        text
    };
    let mut path = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        path.push(if ch == '\\' {
            chars.next().unwrap_or(ch)
        } else {
            ch
        });
    }
    from_path(Path::new(&path))
}

impl Source {
    /// Blocking I/O and decoding: call only on a background executor or worker thread.
    /// Inputs within the upload limit remain opaque and unchanged.
    /// The deadline bounds work between reads, not a kernel-blocked filesystem
    /// operation (for example, FUSE); those cannot be interrupted portably.
    pub fn prepare(self) -> Result<PreparedImage> {
        let (extension, bytes) = match self {
            Self::File { path, extension } => {
                let deadline = Instant::now() + MAX_READ_DURATION;
                let mut options = OpenOptions::new();
                options.read(true);
                // Validate the descriptor, not the path: a FIFO swapped into the
                // path before open must not block waiting for a writer.
                #[cfg(unix)]
                options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
                let mut file = options.open(&path).map_err(|source| Error::ImageFile {
                    operation: "open",
                    source,
                })?;
                let metadata = file.metadata().map_err(|source| Error::ImageFile {
                    operation: "inspect",
                    source,
                })?;
                if !metadata.is_file() {
                    return Err(Error::ImageFileType);
                }
                if metadata.len() > MAX_IMAGE_INPUT_BYTES as u64 {
                    return Err(Error::ImageInputTooLarge {
                        limit: MAX_IMAGE_INPUT_BYTES,
                    });
                }
                if metadata.len() == 0 {
                    return Err(Error::ImageSize);
                }
                let mut bytes = Vec::new();
                let mut chunk = [0; 64 * 1024];
                loop {
                    if Instant::now() >= deadline {
                        return Err(Error::ImageReadTimeout);
                    }
                    // One extra byte detects growth past the metadata size.
                    let limit = chunk.len().min(MAX_IMAGE_INPUT_BYTES + 1 - bytes.len());
                    let read = file.read(&mut chunk[..limit]);
                    if Instant::now() >= deadline {
                        return Err(Error::ImageReadTimeout);
                    }
                    if matches!(&read, Err(source) if source.kind() == io::ErrorKind::Interrupted) {
                        continue;
                    }
                    let count = read.map_err(|source| Error::ImageFile {
                        operation: "read",
                        source,
                    })?;
                    if count == 0 {
                        break;
                    }
                    if bytes.len() + count > MAX_IMAGE_INPUT_BYTES {
                        return Err(Error::ImageInputTooLarge {
                            limit: MAX_IMAGE_INPUT_BYTES,
                        });
                    }
                    bytes.extend_from_slice(&chunk[..count]);
                }
                (extension, bytes)
            }
            Self::Clipboard(image) => {
                let extension = match image.format {
                    ImageFormat::Png => "png",
                    ImageFormat::Jpeg => "jpg",
                    ImageFormat::Gif => "gif",
                    ImageFormat::Webp => "webp",
                    ImageFormat::Bmp => "bmp",
                    ImageFormat::Tiff => "tiff",
                    ImageFormat::Svg | ImageFormat::Ico | ImageFormat::Pnm => {
                        return Err(Error::ImageFormat);
                    }
                };
                (extension, image.bytes)
            }
        };
        prepare_bytes(extension, bytes, MAX_CLIPBOARD_IMAGE_PAYLOAD)
    }
}

fn prepare_bytes(
    extension: &'static str,
    bytes: Vec<u8>,
    output_limit: usize,
) -> Result<PreparedImage> {
    if bytes.len() > MAX_IMAGE_INPUT_BYTES {
        return Err(Error::ImageInputTooLarge {
            limit: MAX_IMAGE_INPUT_BYTES,
        });
    }
    if bytes.is_empty() {
        return Err(Error::ImageSize);
    }
    // TIFF is not a bridge format: agents may not read it and the daemon names
    // unknown extensions `.png`. Transcode it even when it already fits.
    let tiff = extension == "tiff";
    if !tiff && bytes.len() <= output_limit {
        return Ok(PreparedImage {
            extension,
            bytes,
            resized: false,
        });
    }
    if matches!(extension, "gif" | "webp") {
        return Err(Error::ImageAnimationResize);
    }
    // Sniff only oversized input; a renamed animation must not become a still.
    let format = image::guess_format(&bytes).map_err(Error::ImageDecode)?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    limits.max_image_width = Some(MAX_DECODE_PIXELS as u32);
    limits.max_image_height = Some(MAX_DECODE_PIXELS as u32);
    let decoded = match format {
        image::ImageFormat::Gif | image::ImageFormat::WebP => {
            return Err(Error::ImageAnimationResize);
        }
        image::ImageFormat::Png => {
            let decoder = PngDecoder::with_limits(Cursor::new(&bytes), limits.clone())
                .map_err(Error::ImageDecode)?;
            if decoder.is_apng().map_err(Error::ImageDecode)? {
                return Err(Error::ImageAnimationResize);
            }
            decode_limited(decoder, limits)?
        }
        image::ImageFormat::Jpeg | image::ImageFormat::Bmp | image::ImageFormat::Tiff => {
            let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
            reader.limits(limits.clone());
            decode_limited(reader.into_decoder().map_err(Error::ImageDecode)?, limits)?
        }
        _ => return Err(Error::ImageFormat),
    };
    drop(bytes);
    let alpha = decoded.color().has_alpha();
    let mut raster = if alpha {
        DynamicImage::ImageRgba8(decoded.into_rgba8())
    } else {
        DynamicImage::ImageRgb8(decoded.into_rgb8())
    };
    let mut output = BoundedOutput {
        bytes: Vec::with_capacity(output_limit),
        limit: output_limit,
        exceeded: false,
    };
    // A converted screenshot keeps lossless pixels when they fit; only a TIFF
    // that is too large for PNG takes the lossy resize path below.
    if tiff {
        match PngEncoder::new(&mut output).write_image(
            raster.as_bytes(),
            raster.width(),
            raster.height(),
            raster.color().into(),
        ) {
            Ok(()) => {
                return Ok(PreparedImage {
                    extension: "png",
                    bytes: output.bytes,
                    resized: false,
                });
            }
            Err(_) if output.exceeded => {}
            Err(source) => return Err(Error::ImageEncode(source)),
        }
    }
    let extension = if alpha { "png" } else { "jpg" };
    loop {
        for quality in [90, 80, 65] {
            // JPEG dimensions are 16-bit even when the source format is not.
            if !alpha && (raster.width() > u16::MAX.into() || raster.height() > u16::MAX.into()) {
                break;
            }
            output.bytes.clear();
            output.exceeded = false;
            let encoded = if alpha {
                PngEncoder::new(&mut output).write_image(
                    raster.as_bytes(),
                    raster.width(),
                    raster.height(),
                    raster.color().into(),
                )
            } else {
                JpegEncoder::new_with_quality(&mut output, quality).encode_image(&raster)
            };
            match encoded {
                Ok(()) => {
                    return Ok(PreparedImage {
                        extension,
                        bytes: output.bytes,
                        resized: true,
                    });
                }
                Err(_) if output.exceeded => {}
                Err(source) => return Err(Error::ImageEncode(source)),
            }
            if alpha {
                break;
            }
        }
        if raster.width() == 1 && raster.height() == 1 {
            return Err(Error::ImageTooLarge {
                limit: output_limit,
            });
        }
        // Integer thumbnailing avoids the large floating-point scratch buffer
        // used by filtered resizing. Every failed round reduces the pixel count.
        raster = raster.thumbnail((raster.width() / 2).max(1), (raster.height() / 2).max(1));
    }
}

fn decode_limited(mut decoder: impl ImageDecoder, mut limits: Limits) -> Result<DynamicImage> {
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS
        || decoder.total_bytes() > MAX_DECODE_BYTES
    {
        return Err(Error::ImageDecodeLimit);
    }
    let orientation = decoder.orientation().map_err(Error::ImageDecode)?;
    // Quarter turns keep the original raster alive while allocating its rotated
    // copy. Flips and 180-degree rotation are in-place in image::apply_orientation.
    if matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    ) && decoder.total_bytes() > MAX_DECODE_BYTES / 2
    {
        return Err(Error::ImageDecodeLimit);
    }
    // Reserve the destination before giving the decoder its remaining budget.
    // Codec scratch limits are best-effort in image; the raster bound is strict.
    limits
        .reserve(decoder.total_bytes())
        .map_err(Error::ImageDecode)?;
    decoder.set_limits(limits).map_err(Error::ImageDecode)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(Error::ImageDecode)?;
    // Re-encoding drops EXIF, so bake its display transform into the pixels first.
    image.apply_orientation(orientation);
    Ok(image)
}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit - self.bytes.len() {
            self.exceeded = true;
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::{
        error::Error as _,
        fs::{self, File},
    };

    #[test]
    fn recognizes_absolute_paths_without_io_and_canonicalizes_extensions() {
        for (suffix, expected) in [
            ("PNG", "png"),
            ("jPg", "jpg"),
            ("JPEG", "jpg"),
            ("GiF", "gif"),
            ("WebP", "webp"),
            ("BmP", "bmp"),
        ] {
            let path = std::env::temp_dir().join(format!("nonexistent/image.{suffix}"));
            assert!(
                matches!(from_path(&path), Some(Source::File { path: actual, extension })
                if actual == path && extension == expected)
            );
        }
        for path in [
            "image.png",
            "~/image.png",
            "",
            "/image.svg",
            "/image.tiff",
            "/image",
        ] {
            assert!(from_path(Path::new(path)).is_none());
        }
    }

    #[test]
    fn parses_unix_drop_quotes_escapes_and_bracketed_paste() {
        let root = if cfg!(windows) { "C:/tmp" } else { "/tmp" };
        let expected = format!("{root}/a b.png");
        for text in [
            expected.clone(),
            format!("{root}/a\\ b.png\r\n"),
            format!("'{root}/a b.png'"),
            format!("\"{root}/a\\ b.png\"\n"),
            format!("\x1b[200~'{root}/a\\ b.png'\r\n\x1b[201~"),
        ] {
            assert!(
                matches!(from_paste(&text), Some(Source::File { path, extension: "png" })
                if path == Path::new(&expected))
            );
        }
        assert!(
            matches!(from_paste(&format!("'{root}/a\\\\b.png'")), Some(Source::File { path, .. })
            if path == Path::new(&format!("{root}/a\\b.png")))
        );
    }

    #[test]
    fn rejects_controls_multiline_relative_and_oversized_pastes() {
        let root = if cfg!(windows) { "C:/" } else { "/" };
        for text in [
            "",
            "\r\n",
            "image.png",
            "~/image.png",
            "'/tmp/a.png\"",
            "/tmp/a.png\n/tmp/b.png",
            "\x1b[200~/tmp/a.png",
            "/tmp/a.png\x1b[201~",
        ] {
            assert!(from_paste(text).is_none());
        }
        for control in [
            '\0', '\n', '\r', '\t', '\u{1b}', '\u{7f}', '\u{85}', '\u{9b}',
        ] {
            assert!(from_paste(&format!("{root}tmp/a{control}.png")).is_none());
        }
        let at_limit = format!(
            "{root}{}.png",
            "x".repeat(MAX_SOURCE_BYTES - root.len() - 4)
        );
        assert!(from_paste(&at_limit).is_some());
        assert!(from_paste(&format!("{at_limit}\n")).is_none());
        assert!(from_path(Path::new(&format!("/{at_limit}"))).is_none());
    }

    #[test]
    fn file_reads_are_opaque_and_enforce_regular_nonempty_bounded_payloads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        fs::write(&path, b"opaque, not decoded").unwrap();
        let prepared = from_path(&path).unwrap().prepare().unwrap();
        assert_eq!(prepared.extension, "png");
        assert_eq!(prepared.bytes, b"opaque, not decoded");
        assert!(!prepared.resized);
        for length in [0, MAX_IMAGE_INPUT_BYTES as u64 + 1] {
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(length)
                .unwrap();
            assert!(matches!(
                from_path(&path).unwrap().prepare(),
                Err(Error::ImageSize | Error::ImageInputTooLarge { .. })
            ));
        }
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_CLIPBOARD_IMAGE_PAYLOAD as u64)
            .unwrap();
        assert_eq!(
            from_path(&path).unwrap().prepare().unwrap().bytes.len(),
            MAX_CLIPBOARD_IMAGE_PAYLOAD
        );
        let nonregular = directory.path().join("directory.png");
        fs::create_dir(&nonregular).unwrap();
        #[cfg(unix)]
        assert!(matches!(
            from_path(&nonregular).unwrap().prepare(),
            Err(Error::ImageFileType)
        ));
        #[cfg(windows)]
        assert!(matches!(
            from_path(&nonregular).unwrap().prepare(),
            Err(Error::ImageFile { operation: "open", source })
                if source.kind() == io::ErrorKind::PermissionDenied
        ));
    }

    #[test]
    fn file_errors_keep_io_sources_without_displaying_paths_or_diagnostics() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private-missing.png");
        let error = from_path(&path).unwrap().prepare().unwrap_err();
        assert!(matches!(
            error,
            Error::ImageFile {
                operation: "open",
                ..
            }
        ));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!error.to_string().contains("private-missing"));
        let error = Error::ImageFile {
            operation: "read",
            source: io::Error::other("private diagnostic"),
        };
        assert!(!error.to_string().contains("private diagnostic"));
        assert_eq!(error.source().unwrap().to_string(), "private diagnostic");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_file_replaced_by_a_fifo_without_waiting_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        fs::write(&path, b"image").unwrap();
        let source = from_path(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let _ = sender.send(source.prepare());
        });
        let result = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
        assert!(matches!(result, Err(Error::ImageFileType)));
    }

    #[test]
    fn clipboard_formats_pass_bytes_through_without_decoding_or_copying() {
        for (format, extension) in [
            (ImageFormat::Png, "png"),
            (ImageFormat::Jpeg, "jpg"),
            (ImageFormat::Gif, "gif"),
            (ImageFormat::Webp, "webp"),
            (ImageFormat::Bmp, "bmp"),
        ] {
            let image = Image::from_bytes(format, vec![0, 1, 255]);
            let pointer = image.bytes.as_ptr();
            let prepared = Source::Clipboard(image).prepare().unwrap();
            assert_eq!(prepared.extension, extension);
            assert_eq!(prepared.bytes, vec![0, 1, 255]);
            assert_eq!(prepared.bytes.as_ptr(), pointer);
            assert!(!prepared.resized);
        }
        assert!(matches!(
            Source::Clipboard(Image::from_bytes(ImageFormat::Svg, vec![1])).prepare(),
            Err(Error::ImageFormat)
        ));
        let error = Source::Clipboard(Image::from_bytes(ImageFormat::Tiff, vec![1]))
            .prepare()
            .unwrap_err();
        assert!(matches!(error, Error::ImageDecode(_)));
        assert!(matches!(
            Source::Clipboard(Image::from_bytes(ImageFormat::Png, Vec::new())).prepare(),
            Err(Error::ImageSize)
        ));
    }

    #[test]
    fn exact_upload_limit_is_opaque_but_oversized_invalid_images_keep_decode_source() {
        let bytes = vec![0; MAX_CLIPBOARD_IMAGE_PAYLOAD];
        let pointer = bytes.as_ptr();
        let prepared = Source::Clipboard(Image::from_bytes(ImageFormat::Png, bytes))
            .prepare()
            .unwrap();
        assert_eq!(prepared.bytes.len(), MAX_CLIPBOARD_IMAGE_PAYLOAD);
        assert_eq!(prepared.bytes.as_ptr(), pointer);
        assert!(!prepared.resized);
        let error = prepare_bytes(
            "png",
            vec![0; MAX_CLIPBOARD_IMAGE_PAYLOAD + 1],
            MAX_CLIPBOARD_IMAGE_PAYLOAD,
        )
        .unwrap_err();
        assert!(matches!(error, Error::ImageDecode(_)));
        assert!(error.source().unwrap().is::<image::ImageError>());
        assert!(!error.to_string().contains("signature"));
    }

    fn noisy_png(alpha: bool) -> Vec<u8> {
        noisy_image(alpha, image::ImageFormat::Png)
    }

    fn noisy_raster(alpha: bool) -> DynamicImage {
        let mut seed = 1_u32;
        let raster = image::RgbaImage::from_fn(128, 128, |_, _| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut rgba = seed.to_le_bytes();
            rgba[3] = 73;
            image::Rgba(rgba)
        });
        let raster = DynamicImage::ImageRgba8(raster);
        if alpha {
            raster
        } else {
            DynamicImage::ImageRgb8(raster.into_rgb8())
        }
    }

    fn noisy_image(alpha: bool, format: image::ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        noisy_raster(alpha).write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn tiff_clipboard_images_become_lossless_png_even_when_small() {
        for alpha in [false, true] {
            let tiff = noisy_image(alpha, image::ImageFormat::Tiff);
            assert!(tiff.len() < MAX_CLIPBOARD_IMAGE_PAYLOAD);
            let prepared = Source::Clipboard(Image::from_bytes(ImageFormat::Tiff, tiff))
                .prepare()
                .unwrap();
            assert_eq!(prepared.extension, "png");
            assert!(!prepared.resized);
            assert_eq!(
                image::guess_format(&prepared.bytes).unwrap(),
                image::ImageFormat::Png
            );
            let decoded = image::load_from_memory(&prepared.bytes).unwrap();
            assert_eq!(decoded.color().has_alpha(), alpha);
            assert_eq!(decoded.as_bytes(), noisy_raster(alpha).as_bytes());
        }
    }

    #[test]
    fn tiff_too_large_for_png_takes_the_bounded_resize_path() {
        for alpha in [false, true] {
            let output_limit = if alpha { 4096 } else { 1024 };
            let prepared = prepare_bytes(
                "tiff",
                noisy_image(alpha, image::ImageFormat::Tiff),
                output_limit,
            )
            .unwrap();
            assert!(prepared.resized);
            assert!(prepared.bytes.len() <= output_limit);
            assert_eq!(prepared.extension, if alpha { "png" } else { "jpg" });
            let raster = image::load_from_memory(&prepared.bytes).unwrap();
            assert!(raster.width() < 128 && raster.height() < 128);
        }
    }

    #[test]
    fn noisy_images_downscale_with_bounded_output_and_preserve_alpha() {
        for alpha in [false, true] {
            let bytes = noisy_png(alpha);
            let output_limit = if alpha { 4096 } else { 1024 };
            assert!(bytes.len() > output_limit);
            let prepared = prepare_bytes("png", bytes, output_limit).unwrap();
            assert!(prepared.resized);
            assert!(prepared.bytes.len() <= output_limit);
            assert_eq!(prepared.extension, if alpha { "png" } else { "jpg" });
            let raster = image::load_from_memory(&prepared.bytes).unwrap();
            assert!(raster.width() < 128 && raster.height() < 128);
            assert_eq!(raster.color().has_alpha(), alpha);
            if alpha {
                assert!(raster.to_rgba8().pixels().all(|pixel| pixel[3] == 73));
            }
        }
    }

    #[test]
    fn oversized_file_is_recompressed_without_modifying_original() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        let mut original = noisy_png(false);
        original.resize(MAX_CLIPBOARD_IMAGE_PAYLOAD + 1, 0);
        fs::write(&path, &original).unwrap();
        let prepared = from_path(&path).unwrap().prepare().unwrap();
        assert!(prepared.resized);
        assert_eq!(prepared.extension, "jpg");
        assert!(prepared.bytes.len() <= MAX_CLIPBOARD_IMAGE_PAYLOAD);
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn bmp_decoder_is_available_for_resize() {
        let raster = image::load_from_memory(&noisy_png(false)).unwrap();
        let mut bmp = Cursor::new(Vec::new());
        raster.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let prepared = prepare_bytes("bmp", bmp.into_inner(), 1024).unwrap();
        assert_eq!(prepared.extension, "jpg");
        assert!(prepared.resized);
        assert!(prepared.bytes.len() <= 1024);
    }

    #[test]
    fn jpeg_input_is_recompressed_and_png_dimensions_fit_jpeg_before_encoding() {
        let raster = image::load_from_memory(&noisy_png(false)).unwrap();
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 95)
            .encode_image(&raster)
            .unwrap();
        let prepared = prepare_bytes("jpg", jpeg, 1024).unwrap();
        assert!(prepared.resized);
        assert_eq!(prepared.extension, "jpg");
        assert!(prepared.bytes.len() <= 1024);

        let wide = image::RgbImage::new(u32::from(u16::MAX) + 1, 1);
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(
                wide.as_raw(),
                wide.width(),
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        png.resize(64 * 1024 + 1, 0);
        let prepared = prepare_bytes("png", png, 64 * 1024).unwrap();
        let decoded = image::load_from_memory(&prepared.bytes).unwrap();
        assert_eq!(decoded.width(), 32768);
        assert_eq!(decoded.height(), 1);
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(&chunk[4..]);
        chunk.extend_from_slice(&crc.sum().to_be_bytes());
        chunk
    }

    fn orientation_exif(orientation: u8) -> Vec<u8> {
        // Little-endian TIFF with one IFD0 SHORT entry: Orientation (0x0112).
        vec![
            b'I',
            b'I',
            42,
            0,
            8,
            0,
            0,
            0,
            1,
            0,
            0x12,
            1,
            3,
            0,
            1,
            0,
            0,
            0,
            orientation,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]
    }

    #[test]
    fn jpeg_exif_orientation_is_baked_into_reencoded_pixels() {
        let colors = [[240, 20, 20], [20, 240, 20], [20, 20, 240], [240, 240, 20]];
        let raster = image::RgbImage::from_fn(80, 48, |x, y| {
            image::Rgb(colors[usize::from(y >= 24) * 2 + usize::from(x >= 40)])
        });
        // Expected source quadrant at each output corner: TL, TR, BL, BR.
        for (orientation, corners) in [
            (1, [0, 1, 2, 3]),
            (2, [1, 0, 3, 2]),
            (3, [3, 2, 1, 0]),
            (4, [2, 3, 0, 1]),
            (5, [0, 2, 1, 3]),
            (6, [2, 0, 3, 1]),
            (7, [3, 1, 2, 0]),
            (8, [1, 3, 0, 2]),
        ] {
            let mut jpeg = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 95);
            encoder
                .set_exif_metadata(orientation_exif(orientation))
                .unwrap();
            encoder.encode_image(&raster).unwrap();
            let passthrough = prepare_bytes("jpg", jpeg.clone(), jpeg.len()).unwrap();
            assert!(!passthrough.resized);
            assert_eq!(passthrough.bytes, jpeg);

            let output_limit = 8192;
            jpeg.resize(output_limit + 1, 0);
            let prepared = prepare_bytes("jpg", jpeg, output_limit).unwrap();
            assert!(prepared.resized);
            assert_eq!(prepared.extension, "jpg");
            assert!(prepared.bytes.len() <= output_limit);
            let mut decoder =
                ImageReader::with_format(Cursor::new(&prepared.bytes), image::ImageFormat::Jpeg)
                    .into_decoder()
                    .unwrap();
            assert_eq!(decoder.orientation().unwrap(), Orientation::NoTransforms);
            let decoded = DynamicImage::from_decoder(decoder).unwrap().into_rgb8();
            assert_eq!(
                decoded.dimensions(),
                if orientation >= 5 { (48, 80) } else { (80, 48) }
            );
            for ((x, y), corner) in [
                (8, 8),
                (decoded.width() - 9, 8),
                (8, decoded.height() - 9),
                (decoded.width() - 9, decoded.height() - 9),
            ]
            .into_iter()
            .zip(corners)
            {
                for (actual, expected) in decoded.get_pixel(x, y).0.into_iter().zip(colors[corner])
                {
                    assert!(
                        actual.abs_diff(expected) < 15,
                        "orientation {orientation}, corner {corner}"
                    );
                }
            }
        }
    }

    #[test]
    fn quarter_turn_memory_is_bounded_before_decoding() {
        for orientation in 1..=8 {
            let mut png = noisy_png(true);
            let mut header = 8192_u32.to_be_bytes().to_vec();
            header.extend_from_slice(&4097_u32.to_be_bytes());
            header.extend_from_slice(&[8, 6, 0, 0, 0]);
            png.splice(8..33, png_chunk(b"IHDR", &header));
            png.splice(33..33, png_chunk(b"eXIf", &orientation_exif(orientation)));
            let decoder = PngDecoder::with_limits(Cursor::new(&png), Limits::default()).unwrap();
            // A tiny decoder budget stops in-place orientations before allocating;
            // quarter turns must hit our stricter two-raster bound first.
            let mut limits = Limits::default();
            limits.max_alloc = Some(1);
            let error = decode_limited(decoder, limits).unwrap_err();
            if orientation >= 5 {
                assert!(matches!(error, Error::ImageDecodeLimit));
            } else {
                assert!(matches!(
                    error,
                    Error::ImageDecode(image::ImageError::Limits(_))
                ));
            }
        }
    }

    #[test]
    fn oversized_animations_are_rejected_even_with_misleading_extensions() {
        for (extension, bytes) in [
            ("gif", vec![0; 32]),
            ("webp", vec![0; 32]),
            ("png", b"GIF89a01234567890123456789".to_vec()),
            ("jpg", b"RIFF\x10\x00\x00\x00WEBPVP8 0123456789".to_vec()),
        ] {
            assert!(matches!(
                prepare_bytes(extension, bytes, 16),
                Err(Error::ImageAnimationResize)
            ));
        }
        let mut png = noisy_png(true);
        // acTL before IDAT declares animation, even when the default image is
        // only a thumbnail. Its CRC is valid so rejection is not a decode error.
        png.splice(33..33, png_chunk(b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]));
        assert!(matches!(
            prepare_bytes("png", png, 1024),
            Err(Error::ImageAnimationResize)
        ));
    }

    #[test]
    fn pixel_and_decoded_byte_bombs_are_rejected_before_raster_decode() {
        for (width, height, depth, color) in [(8193_u32, 8192_u32, 8, 0), (8192, 8192, 16, 6)] {
            let mut png = noisy_png(true);
            let mut header = width.to_be_bytes().to_vec();
            header.extend_from_slice(&height.to_be_bytes());
            header.extend_from_slice(&[depth, color, 0, 0, 0]);
            png.splice(8..33, png_chunk(b"IHDR", &header));
            assert!(matches!(
                prepare_bytes("png", png, 1024),
                Err(Error::ImageDecodeLimit)
            ));
        }
    }

    #[test]
    fn clipboard_input_cap_precedes_decode_and_accepts_the_boundary() {
        assert!(matches!(
            Source::Clipboard(Image::from_bytes(
                ImageFormat::Png,
                vec![0; MAX_IMAGE_INPUT_BYTES + 1]
            ))
            .prepare(),
            Err(Error::ImageInputTooLarge {
                limit: MAX_IMAGE_INPUT_BYTES
            })
        ));
        assert!(matches!(
            Source::Clipboard(Image::from_bytes(
                ImageFormat::Png,
                vec![0; MAX_IMAGE_INPUT_BYTES]
            ))
            .prepare(),
            Err(Error::ImageDecode(_))
        ));
    }

    #[test]
    fn encoded_output_never_grows_past_limit_and_tiny_targets_terminate() {
        let mut output = BoundedOutput {
            bytes: Vec::with_capacity(4),
            limit: 4,
            exceeded: false,
        };
        output.write_all(&[1, 2, 3, 4]).unwrap();
        assert_eq!(
            output.write_all(&[5]).unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        assert!(output.exceeded);
        assert_eq!(output.bytes, [1, 2, 3, 4]);
        assert_eq!(output.bytes.capacity(), 4);
        assert!(matches!(
            prepare_bytes("png", noisy_png(false), 1),
            Err(Error::ImageTooLarge { limit: 1 })
        ));
    }
}

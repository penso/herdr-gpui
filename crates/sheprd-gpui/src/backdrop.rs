//! What paints behind the main window's terminals and chrome when
//! `[backdrop]` makes the theme's background translucent.
//!
//! The ground — the theme's background at the backdrop's opacity — is painted
//! once, at the window's root, over the desktop or a picture. Layers that
//! would repaint the theme's background over it paint nothing, and terminal
//! cells on the default background skip their quad, so alpha never stacks
//! into a darker band and the grid paints fewer quads than an opaque one.
//! Chrome surfaces keep their own colors at the same opacity over the ground,
//! a little more solid than the terminal so their labels stay readable.
//! Popups, menus, and dialogs stay opaque.
use crate::config::{self, Theme};
use gpui::{Hsla, RenderImage, WindowBackgroundAppearance, rgb, transparent_black};
use image::{DynamicImage, ImageReader, Limits, RgbaImage};
use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

/// Files past this are refused before decoding: no wallpaper needs it.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// What the decoder may allocate, for a picture of at most `MAX_DECODE_SIDE`.
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DECODE_SIDE: u32 = 16_384;
/// The longest side a picture keeps once decoded, a 4K display's width: a
/// larger photo costs no more texture memory than one that fills the screen.
pub(crate) const MAX_TEXTURE_SIDE: u32 = 3_840;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("backdrop.opacity must be finite and between 0 and 1")]
    Opacity,
    #[error("backdrop.image must not be empty")]
    EmptyImage,
    #[error("backdrop.image must be an absolute path or start with ~/")]
    RelativeImage,
    #[error("backdrop.image must be a UTF-8 path")]
    ImagePathEncoding,
    #[error("Could not read the backdrop image {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("The backdrop image {path} is not a regular file")]
    NotAFile { path: PathBuf },
    #[error("The backdrop image {path} exceeds the {MAX_FILE_BYTES}-byte limit")]
    TooLarge { path: PathBuf },
    #[error("Could not decode the backdrop image {path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },
}

/// Decodes the picture at `path` into the BGRA texture GPUI uploads, scaled
/// to fit `MAX_TEXTURE_SIDE`. Blocking: run it on a background thread.
pub(crate) fn decode(path: &Path) -> Result<RenderImage, Error> {
    let read = |source| Error::Read {
        path: path.to_path_buf(),
        source,
    };
    let not_a_file = || Error::NotAFile {
        path: path.to_path_buf(),
    };
    // Checked before opening: opening a FIFO with no writer blocks, and with
    // it the decode and any Settings save waiting on it.
    if !fs::metadata(path).map_err(read)?.is_file() {
        return Err(not_a_file());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    // Should the path become a FIFO after the check, the open still returns.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(
        &mut options,
        rustix::fs::OFlags::NONBLOCK.bits() as i32,
    );
    let file = options.open(path).map_err(read)?;
    if !file.metadata().map_err(read)?.is_file() {
        return Err(not_a_file());
    }
    // Bounded by the read, not the metadata, which can change underneath.
    let mut bytes = Vec::new();
    BufReader::new(file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(read)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::TooLarge {
            path: path.to_path_buf(),
        });
    }
    let decoding = |source| Error::Decode {
        path: path.to_path_buf(),
        source,
    };
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(read)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let picture = fit(reader.decode().map_err(decoding)?);
    Ok(texture(picture.into_rgba8()))
}

/// Scales `picture` down to fit `MAX_TEXTURE_SIDE`, keeping its aspect. The
/// box filter allocates only its output, in the decoded color type, where a
/// convolving one holds a float copy several times the photo's size.
fn fit(picture: DynamicImage) -> DynamicImage {
    let (width, height) = (picture.width(), picture.height());
    if width.max(height) <= MAX_TEXTURE_SIDE {
        return picture;
    }
    let scale = f64::from(MAX_TEXTURE_SIDE) / f64::from(width.max(height));
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).clamp(1, MAX_TEXTURE_SIDE);
    picture.thumbnail_exact(side(width), side(height))
}

fn texture(mut pixels: RgbaImage) -> RenderImage {
    // GPUI keeps pictures as BGRA.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RenderImage::new(vec![image::Frame::new(pixels)])
}

/// The colors one window paints with for its backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fill {
    background: u32,
    /// 1 paints every surface opaque, as without a backdrop.
    alpha: f32,
}

impl Fill {
    pub(crate) fn new(theme: &Theme, backdrop: &config::Backdrop) -> Self {
        Self {
            background: theme.background,
            alpha: if backdrop.translucent() {
                backdrop.opacity
            } else {
                1.
            },
        }
    }

    pub(crate) fn translucent(self) -> bool {
        self.alpha < 1.
    }

    /// How solid a mark lying on a translucent panel paints, such as a
    /// selected row or tab. Nothing can lie on a panel without covering the
    /// desktop more than the panel alone does, so a mark takes the square of
    /// the window's opacity: still seen, close to the panel's clarity, and
    /// opaque with an opaque window.
    pub(crate) fn mark(self) -> f32 {
        self.alpha * self.alpha
    }

    /// The theme's background, translucent with the window: what a region
    /// with nothing else under it paints, such as a group's body.
    pub(crate) fn ground(self) -> Hsla {
        self.tint(rgb(self.background).into())
    }

    /// A layer lying directly on the ground; see [`Self::over`].
    pub(crate) fn base(self, color: u32) -> Hsla {
        self.over(self.background, color)
    }

    /// A layer of `color` over one of `under`. Under a backdrop, a layer the
    /// color of what it lies on paints nothing, so the two read as one
    /// surface rather than a darker band; any other color is a [`Self::mark`].
    pub(crate) fn over(self, under: u32, color: u32) -> Hsla {
        if self.translucent() && color == under {
            transparent_black()
        } else {
            Hsla::from(rgb(color)).opacity(self.mark())
        }
    }

    /// A surface of its own color, translucent with the window. It must lie
    /// on nothing else translucent, or the two cover the desktop twice.
    pub(crate) fn panel(self, color: u32) -> Hsla {
        self.tint(rgb(color).into())
    }

    pub(crate) fn tint(self, color: Hsla) -> Hsla {
        color.opacity(self.alpha)
    }
}

/// The platform appearance for a backdrop: a picture covers the desktop, so
/// the window stays opaque and the compositor has nothing to blend. Only
/// Windows has Mica; elsewhere the material is ignored and blur is blur.
pub(crate) fn appearance(backdrop: &config::Backdrop, picture: bool) -> WindowBackgroundAppearance {
    match (backdrop.translucent(), picture, backdrop.blur) {
        (false, ..) | (true, true, _) => WindowBackgroundAppearance::Opaque,
        (true, false, true) if cfg!(windows) && backdrop.material == config::Material::Mica => {
            WindowBackgroundAppearance::MicaBackdrop
        }
        (true, false, true) => WindowBackgroundAppearance::Blurred,
        (true, false, false) => WindowBackgroundAppearance::Transparent,
    }
}

/// A window's picture: the one it paints and the one it is decoding.
#[derive(Default)]
pub(crate) struct Picture {
    shown: Option<(PathBuf, Arc<RenderImage>)>,
    /// The path last asked for, decoded or not; a failure is not retried
    /// until the path changes.
    wanted: Option<PathBuf>,
    pending: bool,
    /// Textures no longer painted, for the window to drop from its atlas.
    released: Vec<Arc<RenderImage>>,
}

impl Picture {
    /// Records the path the config asks for and whether a decode should
    /// start for it. A path that went away releases its texture at once.
    pub(crate) fn want(&mut self, path: Option<PathBuf>) -> Option<PathBuf> {
        if path == self.wanted {
            return None;
        }
        self.wanted = path.clone();
        if self.shown.as_ref().map(|(shown, _)| shown) != path.as_ref()
            && let Some((_, texture)) = self.shown.take()
        {
            self.released.push(texture);
        }
        self.pending = path.is_some();
        path
    }

    /// Applies a finished decode unless the config moved on meanwhile.
    pub(crate) fn finish(&mut self, path: PathBuf, result: Result<RenderImage, Error>) {
        if self.wanted.as_ref() != Some(&path) {
            return;
        }
        self.pending = false;
        match result {
            Ok(texture) => {
                if let Some((_, old)) = self.shown.replace((path, Arc::new(texture))) {
                    self.released.push(old);
                }
            }
            Err(error) => tracing::warn!(%error, "Could not show the backdrop picture"),
        }
    }

    pub(crate) fn texture(&self) -> Option<Arc<RenderImage>> {
        self.shown.as_ref().map(|(_, texture)| texture.clone())
    }

    /// Whether a picture is shown or still decoding; either way the window
    /// keeps the desktop hidden rather than flash it before the picture.
    pub(crate) fn covers(&self) -> bool {
        self.shown.is_some() || self.pending
    }

    pub(crate) fn take_released(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.released)
    }
}

#[cfg(test)]
mod tests;

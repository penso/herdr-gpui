//! A page's favicon, shown on its tab in place of the globe. A script the
//! app runs in each loaded page draws the icon at a fixed size and posts its
//! pixels; WebKit has no favicon API, and pixels the page has already decoded
//! leave the app no image format to parse.
use gpui::RenderImage;
use serde::Deserialize;

const SCRIPT: &str = include_str!("favicon.js");

/// Pixels a side: crisp at the tab's 12 points on a 2x display.
const SIZE: u32 = 32;

/// The least time between two reports of an icon the page swapped.
const PAUSE_MS: u32 = 500;

/// The script that reports the page's icon, to run once it loads.
pub(crate) fn script() -> String {
    format!("{}({SIZE}, {PAUSE_MS});", SCRIPT.trim_end())
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Message {
    Favicon { pixels: Option<Vec<u8>> },
}

/// What a page reported about its icon.
pub(crate) enum Favicon {
    /// Ready for GPUI to paint.
    Icon(RenderImage),
    /// It has none the app can read.
    Missing,
}

impl Favicon {
    /// Parses a favicon report, or `None` for anything else the page
    /// posted, including a malformed report: any script in the page can post.
    pub(crate) fn parse(body: &str) -> Option<Self> {
        let Message::Favicon { pixels } = serde_json::from_str(body).ok()?;
        let Some(pixels) = pixels else {
            return Some(Self::Missing);
        };
        // A longer buffer would still make an image, of its first pixels.
        if pixels.len() != (SIZE * SIZE * 4) as usize {
            return None;
        }
        let mut pixels = image::RgbaImage::from_raw(SIZE, SIZE, pixels)?;
        // GPUI keeps pictures as BGRA.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        Some(Self::Icon(RenderImage::new(vec![image::Frame::new(
            pixels,
        )])))
    }
}

#[cfg(test)]
mod tests;

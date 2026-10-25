//! `[zen]`: what Toggle Zen Mode does beyond clearing the window's chrome.
use super::Error;
use super::FONT_SIZE_RANGE;
use crate::Result;
use serde::Deserialize;

/// The narrowest column zen mode centres; narrower reads as a typo.
const MIN_COLUMNS: u16 = 20;
const MAX_COLUMNS: u16 = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Zen {
    /// Also enter native full screen, and leave it again on the way out
    /// unless the window was already full screen.
    pub fullscreen: bool,
    /// The centred column zen mode draws in; the whole width when unset.
    pub width: Option<ZenWidth>,
    /// The terminal font size while in zen mode, the previous one coming
    /// back on the way out; unchanged when unset.
    pub font_size: Option<f32>,
}

/// How wide zen mode's centred column is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZenWidth {
    /// So many terminal columns at the current font.
    Columns(u16),
    /// A share of the window, from 10 to 100 percent.
    Percent(u8),
}

/// `width` as written: a column count, or a percentage such as `"80%"`.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum RawWidth {
    Columns(i64),
    Text(String),
}

impl TryFrom<RawWidth> for ZenWidth {
    type Error = crate::Error;

    fn try_from(raw: RawWidth) -> Result<Self> {
        let width = match raw {
            RawWidth::Columns(columns) => u16::try_from(columns)
                .ok()
                .filter(|columns| (MIN_COLUMNS..=MAX_COLUMNS).contains(columns))
                .map(Self::Columns),
            RawWidth::Text(text) => text
                .trim()
                .strip_suffix('%')
                .and_then(|share| share.trim().parse::<u8>().ok())
                .filter(|share| (10..=100).contains(share))
                .map(Self::Percent),
        };
        width.ok_or(Error::InvalidZenWidth.into())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct ZenSettings {
    fullscreen: bool,
    width: Option<RawWidth>,
    font_size: Option<f32>,
}

impl TryFrom<ZenSettings> for Zen {
    type Error = crate::Error;

    fn try_from(settings: ZenSettings) -> Result<Self> {
        if let Some(size) = settings.font_size
            && !(size.is_finite() && FONT_SIZE_RANGE.contains(&size))
        {
            return Err(Error::InvalidZenFontSize.into());
        }
        Ok(Self {
            fullscreen: settings.fullscreen,
            width: settings.width.map(ZenWidth::try_from).transpose()?,
            font_size: settings.font_size,
        })
    }
}

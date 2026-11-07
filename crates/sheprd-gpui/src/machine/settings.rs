//! What a machine is given: its CPUs, memory, and display. They take effect
//! when it next boots, as a saved machine can only resume as it was saved.

use super::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, ops::RangeInclusive, str::FromStr};

/// macOS itself needs this much.
const MIN_CPUS: u16 = 2;
const MIN_MEMORY_GIB: u16 = 4;
/// Left to the host, so a machine cannot starve the app showing it.
const HOST_MEMORY_GIB: u16 = 4;
const MAX_DISPLAY_SIDE: u16 = 8192;
const MIN_DISPLAY_SIDE: u16 = 640;

/// How large the guest's screen is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) enum Display {
    /// The screen follows the tab's size.
    #[default]
    Fit,
    /// A fixed size in pixels, which the tab scales to fit.
    Fixed { width: u16, height: u16 },
}

impl Display {
    /// The fixed sizes offered beside [`Self::Fit`].
    pub(crate) const PRESETS: [Self; 3] = [
        Self::Fixed {
            width: 1920,
            height: 1200,
        },
        Self::Fixed {
            width: 2560,
            height: 1600,
        },
        Self::Fixed {
            width: 3024,
            height: 1964,
        },
    ];
}

impl fmt::Display for Display {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fit => formatter.write_str("fit"),
            Self::Fixed { width, height } => write!(formatter, "{width}x{height}"),
        }
    }
}

impl FromStr for Display {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        if text == "fit" {
            return Ok(Self::Fit);
        }
        let (width, height) = text.split_once('x').ok_or(Error::InvalidDisplay)?;
        let side = |text: &str| {
            text.parse::<u16>()
                .ok()
                .filter(|side| (MIN_DISPLAY_SIDE..=MAX_DISPLAY_SIDE).contains(side))
                .ok_or(Error::InvalidDisplay)
        };
        Ok(Self::Fixed {
            width: side(width)?,
            height: side(height)?,
        })
    }
}

impl TryFrom<String> for Display {
    type Error = Error;

    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

impl From<Display> for String {
    fn from(display: Display) -> Self {
        display.to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) cpus: u16,
    pub(crate) memory_gib: u16,
    pub(crate) display: Display,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cpus: 4,
            memory_gib: 8,
            display: Display::Fit,
        }
    }
}

/// What this Mac can give a machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) cpus: RangeInclusive<u16>,
    pub(crate) memory_gib: RangeInclusive<u16>,
}

impl Limits {
    /// The limits of a host with `cpus` cores and `memory_bytes` of memory.
    pub(crate) fn of(cpus: usize, memory_bytes: u64) -> Self {
        let gib = u16::try_from(memory_bytes >> 30).unwrap_or(u16::MAX);
        let cpus = u16::try_from(cpus).unwrap_or(u16::MAX);
        Self {
            cpus: MIN_CPUS..=cpus.max(MIN_CPUS),
            memory_gib: MIN_MEMORY_GIB..=gib.saturating_sub(HOST_MEMORY_GIB).max(MIN_MEMORY_GIB),
        }
    }

    /// The limits of this Mac.
    pub(crate) fn host() -> Self {
        let host = objc2_foundation::NSProcessInfo::processInfo();
        Self::of(host.activeProcessorCount(), host.physicalMemory())
    }

    pub(crate) fn check(&self, settings: &Settings) -> Result<()> {
        if !self.cpus.contains(&settings.cpus) {
            return Err(Error::Cpus {
                min: *self.cpus.start(),
                max: *self.cpus.end(),
            });
        }
        if !self.memory_gib.contains(&settings.memory_gib) {
            return Err(Error::Memory {
                min: *self.memory_gib.start(),
                max: *self.memory_gib.end(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

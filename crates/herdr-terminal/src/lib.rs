//! A pure terminal emulator: raw PTY bytes in, Herdr's cell model out.
#![doc = include_str!("../README.md")]

mod cells;
mod damage;
mod emulator;
mod error;
pub mod input;
mod listener;
mod modes;
mod scan;

pub use damage::Damage;
pub use emulator::{Emulator, Event, Scroll, ScrollMetrics};
pub use error::{Error, Result};
pub use herdr_backend::{MAX_CELLS, MAX_SIDE, Size};
pub use modes::{Modes, MouseEncoding, MouseReporting};

// The `CellData::modifier` bits, as the app's painter reads them.
// keep-sorted start
pub const BOLD: u16 = 1;
pub const DIM: u16 = 1 << 1;
pub const HIDDEN: u16 = 1 << 7;
pub const ITALIC: u16 = 1 << 2;
pub const REVERSED: u16 = 1 << 6;
pub const STRIKETHROUGH: u16 = 1 << 8;
pub const UNDERLINE: u16 = 1 << 3;
// keep-sorted end

/// The most history lines a terminal keeps, whatever was asked for.
pub const MAX_SCROLLBACK_LINES: usize = 100_000;
/// The most history cells a terminal keeps: wide terminals keep fewer lines.
pub const MAX_SCROLLBACK_CELLS: usize = 10_000_000;
/// The longest OSC string (title, hyperlink) the emulator reads; longer ones
/// are dropped whole rather than buffered without bound.
pub const MAX_OSC_BYTES: usize = 8 * 1024;

#[cfg(test)]
mod tests;

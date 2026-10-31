#![allow(clippy::unwrap_used, clippy::expect_used)]

mod cursor;
mod damage;
mod events;
mod fragmentation;
mod links;
mod scroll;
mod size;
mod style;
mod text;

use super::*;
use herdr_protocol::{CellData, FrameData};

pub(crate) fn size(cols: u16, rows: u16) -> Size {
    Size::new(cols, rows).unwrap()
}

pub(crate) fn emulator(cols: u16, rows: u16) -> Emulator {
    Emulator::new(size(cols, rows), 1_000)
}

pub(crate) fn fed(cols: u16, rows: u16, bytes: &[u8]) -> Emulator {
    let mut emulator = emulator(cols, rows);
    emulator.feed(bytes);
    emulator
}

/// A row's text, with trailing blanks trimmed.
pub(crate) fn line(frame: &FrameData, row: u16) -> String {
    let width = usize::from(frame.width);
    let start = usize::from(row) * width;
    let text: String = frame.cells[start..start + width]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    text.trim_end().to_owned()
}

pub(crate) fn cell(frame: &FrameData, x: u16, y: u16) -> &CellData {
    &frame.cells[usize::from(y) * usize::from(frame.width) + usize::from(x)]
}

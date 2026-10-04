use super::*;
use core::prelude::v1::test;

mod cells;
mod composition;
mod diagnostics;
mod glyph_cache;
mod placed_images;

fn cell(symbol: &str) -> CellData {
    CellData {
        symbol: symbol.into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    }
}

use super::*;
use core::prelude::v1::test;

mod cells;
mod composition;
mod glyph_cache;
mod placed_graphics;
mod timing;

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

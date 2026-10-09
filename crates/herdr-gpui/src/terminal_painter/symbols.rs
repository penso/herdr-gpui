//! Cells only an icon font can draw.
//!
//! Prompts take their icons from the Private Use Area, which no text face
//! covers. Without an installed icon font those cells shape to the platform's
//! missing-glyph box, and nothing on screen says why.
use super::graphics::CellSeparator;
use herdr_client::protocol::FrameData;

/// Whether drawing `symbol` takes an icon font. The solid prompt separators
/// are painted as paths and need none. U+F8FF is left out: it is Apple's own
/// logo, which the system faces of the platform that uses it already carry.
pub(super) fn needs_symbol_font(symbol: &str) -> bool {
    // Every codepoint below is at least three UTF-8 bytes; this rejects ASCII,
    // most of any grid, before decoding.
    if symbol.len() < 3 {
        return false;
    }
    let private_use = symbol
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, '\u{e000}'..='\u{f8fe}' | '\u{f0000}'..='\u{ffffd}'));
    private_use && CellSeparator::from_symbol(symbol).is_none()
}

/// Whether any cell of `frame` takes an icon font to draw.
pub(crate) fn frame_needs_symbol_font(frame: &FrameData) -> bool {
    frame
        .cells
        .iter()
        .any(|cell| !cell.skip && needs_symbol_font(&cell.symbol))
}

//! Converting alacritty's grid into Herdr's cell model: colors keep the
//! program's choice (named, indexed, or RGB) so the app's theme resolves
//! them, and styles become the painter's modifier bits.

use crate::{BOLD, DIM, HIDDEN, ITALIC, REVERSED, STRIKETHROUGH, UNDERLINE, listener::Listener};
use alacritty_terminal::{
    Term,
    grid::Dimensions,
    index::{Column, Line},
    term::{
        cell::{Cell, Flags},
        point_to_viewport,
    },
    vte::ansi::{Color, CursorShape, NamedColor},
};
use herdr_protocol::{CellData, CursorState, FrameData};

/// The color encoding's top bytes (see the crate README).
const INDEXED: u32 = 1 << 24;
const RGB: u32 = 2 << 24;
/// The default foreground or background: the theme's own.
const DEFAULT_COLOR: u32 = 0;

/// The hyperlink table rows index into. A frame builds a fresh one; rows
/// patched onto a frame may only use the URIs that frame already sent.
pub(crate) struct Links<'a> {
    uris: &'a mut Vec<String>,
    growable: bool,
}

impl<'a> Links<'a> {
    pub(crate) fn growable(uris: &'a mut Vec<String>) -> Self {
        uris.clear();
        Self {
            uris,
            growable: true,
        }
    }

    pub(crate) fn fixed(uris: &'a mut Vec<String>) -> Self {
        Self {
            uris,
            growable: false,
        }
    }

    /// The index of `uri`, or `None` when a fixed table lacks it.
    fn index(&mut self, uri: &str) -> Option<u32> {
        if let Some(index) = self.uris.iter().position(|known| known == uri) {
            return u32::try_from(index).ok();
        }
        if !self.growable {
            return None;
        }
        let index = u32::try_from(self.uris.len()).ok()?;
        self.uris.push(uri.to_owned());
        Some(index)
    }
}

/// The viewport, with hyperlinks numbered into `uris`.
pub(crate) fn frame(term: &Term<Listener>, uris: &mut Vec<String>) -> FrameData {
    let mut links = Links::growable(uris);
    let rows = term.screen_lines();
    let mut cells = Vec::with_capacity(rows * term.columns());
    for line in 0..rows {
        // A growable table always has room, so every row converts.
        if let Some(row) = row(term, line, &mut links) {
            cells.extend(row);
        }
    }
    FrameData {
        cells,
        width: side(term.columns()),
        height: side(rows),
        cursor: cursor(term),
        hyperlinks: links.uris.clone(),
        graphics: Vec::new(),
    }
}

/// One viewport row, or `None` when it links a URI the table lacks.
pub(crate) fn row(term: &Term<Listener>, line: usize, links: &mut Links) -> Option<Vec<CellData>> {
    let grid = term.grid();
    let line = Line(i32::try_from(line).ok()? - i32::try_from(grid.display_offset()).ok()?);
    let row = &grid[line];
    (0..grid.columns())
        .map(|column| cell(&row[Column(column)], links))
        .collect()
}

fn cell(cell: &Cell, links: &mut Links) -> Option<CellData> {
    let (fg, dim) = color(cell.fg);
    let (bg, _) = color(cell.bg);
    let hyperlink = match cell.hyperlink() {
        Some(link) => Some(links.index(link.uri())?),
        None => None,
    };
    let skip = cell.flags.contains(Flags::WIDE_CHAR_SPACER);
    let symbol = if skip {
        String::new()
    } else if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) || cell.c == '\t' {
        " ".to_owned()
    } else {
        let mut symbol = String::from(cell.c);
        symbol.extend(cell.zerowidth().into_iter().flatten());
        symbol
    };
    Some(CellData {
        symbol,
        fg,
        bg,
        modifier: modifier(cell.flags) | if dim { DIM } else { 0 },
        skip,
        hyperlink,
    })
}

fn modifier(flags: Flags) -> u16 {
    [
        (Flags::BOLD, BOLD),
        (Flags::DIM, DIM),
        (Flags::ITALIC, ITALIC),
        (Flags::ALL_UNDERLINES, UNDERLINE),
        (Flags::INVERSE, REVERSED),
        (Flags::HIDDEN, HIDDEN),
        (Flags::STRIKEOUT, STRIKETHROUGH),
    ]
    .into_iter()
    .filter(|(flag, _)| flags.intersects(*flag))
    .fold(0, |bits, (_, bit)| bits | bit)
}

/// The encoded color, and whether it was one of alacritty's dim colors,
/// which the painter shows as its normal color drawn dim.
pub(crate) fn color(color: Color) -> (u32, bool) {
    match color {
        Color::Spec(rgb) => (
            RGB | u32::from(rgb.r) << 16 | u32::from(rgb.g) << 8 | u32::from(rgb.b),
            false,
        ),
        Color::Indexed(index) => (INDEXED | u32::from(index), false),
        Color::Named(named) => named_color(named),
    }
}

fn named_color(named: NamedColor) -> (u32, bool) {
    use NamedColor::*;
    let palette = |index: u32| index + 1;
    match named {
        Black => (palette(0), false),
        Red => (palette(1), false),
        Green => (palette(2), false),
        Yellow => (palette(3), false),
        Blue => (palette(4), false),
        Magenta => (palette(5), false),
        Cyan => (palette(6), false),
        White => (palette(7), false),
        BrightBlack => (palette(8), false),
        BrightRed => (palette(9), false),
        BrightGreen => (palette(10), false),
        BrightYellow => (palette(11), false),
        BrightBlue => (palette(12), false),
        BrightMagenta => (palette(13), false),
        BrightCyan => (palette(14), false),
        BrightWhite => (palette(15), false),
        DimBlack => (palette(0), true),
        DimRed => (palette(1), true),
        DimGreen => (palette(2), true),
        DimYellow => (palette(3), true),
        DimBlue => (palette(4), true),
        DimMagenta => (palette(5), true),
        DimCyan => (palette(6), true),
        DimWhite => (palette(7), true),
        DimForeground => (DEFAULT_COLOR, true),
        Foreground | Background | Cursor | BrightForeground => (DEFAULT_COLOR, false),
    }
}

/// The cursor in viewport cells, or `None` while it is scrolled out of view.
/// Shapes are DECSCUSR parameters, as the painter reads them: 0 is the
/// user's default, 1/2 block, 3/4 underline, 5/6 bar (odd ones blink).
pub(crate) fn cursor(term: &Term<Listener>) -> Option<CursorState> {
    let content = term.renderable_content();
    let point = point_to_viewport(content.display_offset, content.cursor.point)?;
    if point.line >= term.screen_lines() || point.column.0 >= term.columns() {
        return None;
    }
    let style = term.cursor_style();
    let steady = u8::from(!style.blinking);
    let (visible, shape) = match content.cursor.shape {
        CursorShape::Hidden => (false, 0),
        // No program set a style: the emulator's default is hollow, which
        // DECSCUSR cannot pick, so it stands for the user's own.
        CursorShape::HollowBlock => (true, 0),
        CursorShape::Block => (true, 1 + steady),
        CursorShape::Underline => (true, 3 + steady),
        CursorShape::Beam => (true, 5 + steady),
    };
    Some(CursorState {
        x: side(point.column.0),
        y: side(point.line),
        visible,
        shape,
    })
}

/// A grid side as a wire value; sizes are checked against `MAX_SIDE`.
pub(crate) fn side(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

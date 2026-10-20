//! Where a word read from printed text continues on the rows around it.
//!
//! Two wraps continue a word: the terminal's own, which breaks a row at the
//! pane's last column and goes on at the next row's first, and an
//! application's word wrap, as Ink's `wrap-ansi` draws it, which breaks a
//! word too long for its block at the block's wrap column and goes on under
//! the row's text. A block lies between walls: the borders of a box or a
//! table cell, or the pane's own edges. Only the rows of the word under the
//! pointer are read; the link readers then tokenize the joined text as they
//! would one row.

use super::super::selection::shown;
use super::{MAX_PATH_BYTES, MAX_ROW_BYTES};
use herdr_client::protocol::{CellData, FrameData};
use std::ops::Range;
use unicode_width::UnicodeWidthStr;

/// The most rows one link spans; a longer one is no link.
const MAX_ROWS: usize = 16;
/// The most bytes of text the rows of one link hold.
const MAX_JOINED_BYTES: usize = 2 * MAX_PATH_BYTES;
/// How many rows each way are scanned for a block's wrap column.
const BLOCK_SCAN: u16 = 16;
/// The most blank cells between a block's wrap column and its right wall.
const MAX_PADDING: u16 = 2;
/// The most cells a list marker or line number before a row's text takes.
const MAX_MARKER: u16 = 3;

/// The frame cells links are read from: a pane's inner rectangle, or a
/// whole popup. Nothing outside them is ever read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bounds {
    pub(super) columns: Range<u16>,
    pub(super) rows: Range<u16>,
}

/// Whether `c` draws a frame: box drawing, block elements, or `|`.
pub(super) fn frame_char(c: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&c) || c == '|'
}

/// How a row continues onto the one below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Join {
    /// The terminal's wrap, read exactly as drawn.
    Raw,
    /// An application's word wrap, continued under the row's text.
    App,
}

/// One row of a run: the columns of it read, and how the row below
/// continues it, if it does.
#[derive(Debug)]
struct Piece {
    row: u16,
    columns: Range<u16>,
    below: Option<Join>,
}

/// A read cell: where its text starts in the joined text, and where it is.
#[derive(Debug, Clone, Copy)]
struct Placed {
    byte: usize,
    row: u16,
    column: u16,
    continuation: bool,
}

/// The text of the rows a word spans, read as one, and the cells it came
/// from.
#[derive(Debug)]
pub(super) struct Joined {
    pub(super) text: String,
    cells: Vec<Placed>,
    /// Where each row a raw wrap continues starts in `text`.
    raw_joins: Vec<usize>,
    /// Whether the last row read is the bottom of the bounds, where the text
    /// may continue off-screen.
    pub(super) ends_at_bottom: bool,
}

impl Joined {
    /// Where the cell at `row` and `column` starts in the text.
    pub(super) fn byte_at(&self, row: u16, column: u16) -> Option<usize> {
        self.cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
            .map(|cell| cell.byte)
    }

    /// Whether the text in `range` runs across a raw wrap.
    pub(super) fn crosses_raw_join(&self, range: &Range<usize>) -> bool {
        self.raw_joins
            .iter()
            .any(|&join| range.start < join && join < range.end)
    }

    /// Whether the word holding `range` is cut off with an ellipsis, the way
    /// an application shortens text that does not fit.
    pub(super) fn cut_off(&self, range: &Range<usize>) -> bool {
        self.text[range.start..]
            .split(char::is_whitespace)
            .next()
            .is_some_and(|word| word.contains('…'))
    }

    /// The frame cells the text in `range` covers, one `(row, columns)` per
    /// row, a wide glyph's continuation cells included.
    pub(super) fn segments(&self, range: &Range<usize>) -> Option<Vec<(u16, Range<u16>)>> {
        let first = self
            .cells
            .iter()
            .position(|cell| !cell.continuation && cell.byte >= range.start)?;
        let last = self
            .cells
            .iter()
            .rposition(|cell| !cell.continuation && cell.byte < range.end)?;
        let row = self.cells[last].row;
        let mut end = last + 1;
        while self
            .cells
            .get(end)
            .is_some_and(|cell| cell.continuation && cell.row == row)
        {
            end += 1;
        }
        let mut rows: Vec<(u16, Range<u16>)> = Vec::new();
        for cell in self.cells.get(first..end)? {
            match rows.last_mut() {
                Some((row, columns)) if *row == cell.row => columns.end = cell.column + 1,
                _ => rows.push((cell.row, cell.column..cell.column + 1)),
            }
        }
        Some(rows)
    }
}

/// The rows the word at `column` of `row` spans, joined; `None` past the
/// row or byte cap, which is never read as a link cut short.
pub(super) fn read(frame: &FrameData, bounds: &Bounds, row: u16, column: u16) -> Option<Joined> {
    let grid = Grid { frame, bounds };
    let mut pieces = vec![Piece {
        row,
        columns: bounds.columns.clone(),
        below: None,
    }];
    if grid.content(row, column) {
        let word = grid.word(row, column);
        grid.walk_down(&mut pieces, word.clone())?;
        grid.walk_up(&mut pieces, word)?;
    }
    grid.joined(&pieces)
}

/// The rows an explicit hyperlink covers, starting from its `columns` on
/// `row`: it goes on at the next row's first cell when that carries the same
/// link, and back onto the row above when it starts a row. `same` says
/// whether a cell carries the link.
pub(super) fn hyperlink_rows(
    frame: &FrameData,
    bounds: &Bounds,
    row: u16,
    columns: Range<u16>,
    same: impl Fn(&CellData) -> bool,
) -> Vec<(u16, Range<u16>)> {
    let grid = Grid { frame, bounds };
    let (start, end) = (bounds.columns.start, bounds.columns.end);
    let carries = |row: u16, column: u16| grid.cell(row, column).is_some_and(&same);
    let on_link = |row: u16, column: u16| {
        grid.cell(row, column)
            .is_some_and(|cell| cell.skip || same(cell))
    };
    let mut rows = vec![(row, columns)];
    while let Some((last, covered)) = rows.last()
        && covered.end == end
        && let Some(next) = last.checked_add(1)
        && carries(next, start)
    {
        let stop = (start..end).find(|&c| !on_link(next, c)).unwrap_or(end);
        rows.push((next, start..stop));
    }
    while let Some((first, covered)) = rows.first()
        && covered.start == start
        && let Some(above) = first.checked_sub(1)
        && carries(above, end - 1)
    {
        let from = (start..end)
            .rev()
            .find(|&c| !on_link(above, c))
            .map_or(start, |c| c + 1);
        rows.insert(0, (above, from..end));
    }
    rows
}

/// The walls around a word: the nearest frame characters beside it that
/// the row it may join also draws in the same column. `None` is the pane's
/// edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Walls {
    left: Option<u16>,
    right: Option<u16>,
}

struct Grid<'a> {
    frame: &'a FrameData,
    bounds: &'a Bounds,
}

impl Grid<'_> {
    fn cell(&self, row: u16, column: u16) -> Option<&CellData> {
        if !self.bounds.rows.contains(&row)
            || !self.bounds.columns.contains(&column)
            || column >= self.frame.width
        {
            return None;
        }
        let index = usize::from(row) * usize::from(self.frame.width) + usize::from(column);
        self.frame.cells.get(index)
    }

    fn blank(&self, row: u16, column: u16) -> bool {
        self.cell(row, column)
            .is_none_or(|cell| !cell.skip && shown(cell).trim().is_empty())
    }

    fn frame_at(&self, row: u16, column: u16) -> bool {
        self.cell(row, column).is_some_and(|cell| {
            let mut chars = shown(cell).chars();
            !cell.skip && chars.next().is_some_and(frame_char) && chars.next().is_none()
        })
    }

    fn content(&self, row: u16, column: u16) -> bool {
        !self.blank(row, column) && !self.frame_at(row, column)
    }

    /// The run of text cells around `column`, bounded by blanks and frames.
    fn word(&self, row: u16, column: u16) -> Range<u16> {
        let mut start = column;
        while start > self.bounds.columns.start && self.content(row, start - 1) {
            start -= 1;
        }
        let mut end = column + 1;
        while end < self.bounds.columns.end && self.content(row, end) {
            end += 1;
        }
        start..end
    }

    fn walls(&self, row: u16, word: &Range<u16>, other: u16) -> Walls {
        let wall = |column: &u16| self.frame_at(row, *column) && self.frame_at(other, *column);
        Walls {
            left: (self.bounds.columns.start..word.start).rev().find(wall),
            right: (word.end..self.bounds.columns.end).find(wall),
        }
    }

    /// The columns between `walls`.
    fn inner(&self, walls: Walls) -> Range<u16> {
        walls
            .left
            .map_or(self.bounds.columns.start, |left| left + 1)
            ..walls.right.unwrap_or(self.bounds.columns.end)
    }

    fn content_start(&self, row: u16, inner: &Range<u16>) -> Option<u16> {
        inner.clone().find(|&column| self.content(row, column))
    }

    fn content_end(&self, row: u16, inner: &Range<u16>) -> Option<u16> {
        inner
            .clone()
            .rev()
            .find(|&column| self.content(row, column))
    }

    /// How the row below continues `word` on `row`, and the column the
    /// continuation starts at.
    fn join_down(&self, row: u16, word: &Range<u16>) -> Option<(Join, u16)> {
        let next = row.checked_add(1)?;
        if !self.bounds.rows.contains(&next) {
            return None;
        }
        self.raw_join(row, word)
            .map(|start| (Join::Raw, start))
            .or_else(|| self.app_join(row, word).map(|start| (Join::App, start)))
    }

    /// The terminal's wrap: no walls, the word reaches the last column, or
    /// stops one short where a wide glyph did not fit, and the next row's
    /// first cell holds text.
    fn raw_join(&self, row: u16, word: &Range<u16>) -> Option<u16> {
        let (start, end) = (self.bounds.columns.start, self.bounds.columns.end);
        let next = row + 1;
        let unwalled = self.walls(row, word, next)
            == Walls {
                left: None,
                right: None,
            };
        let wide_next = || {
            self.cell(next, start + 1).is_some_and(|cell| cell.skip)
                || self
                    .cell(next, start)
                    .is_some_and(|cell| shown(cell).width() > 1)
        };
        let at_edge =
            word.end == end || (word.end + 1 == end && self.blank(row, end - 1) && wide_next());
        (unwalled && at_edge && self.content(next, start)).then_some(start)
    }

    /// An application's word wrap: the next row, in the same block, starts
    /// under this row's text, this word ends at the block's wrap column, and
    /// `wrap-ansi` would have started the word on this row.
    fn app_join(&self, row: u16, word: &Range<u16>) -> Option<u16> {
        let next = row + 1;
        let walls = self.walls(row, word, next);
        let inner = self.inner(walls);
        let next_start = self.content_start(next, &inner)?;
        if !self.continues_under(row, next, &inner, next_start) {
            return None;
        }
        let wrap = self.wrap_column(row, walls, &inner);
        if word.end - 1 != wrap || inner.end - wrap - 1 > MAX_PADDING {
            return None;
        }
        if self.in_table(row, walls) && !self.outside_blank(next, walls) {
            return None;
        }
        let width = (wrap + 1).checked_sub(next_start)?;
        let length = self.word_length(row, word, wrap, &inner);
        breaks_here(word.end - word.start, length, width).then_some(next_start)
    }

    /// Whether `next` starts at `row`'s text, or at the text after the
    /// marker that starts `row`, with nothing but blanks, or the frames
    /// `row` draws too, before it.
    fn continues_under(&self, row: u16, next: u16, inner: &Range<u16>, next_start: u16) -> bool {
        let Some(start) = self.content_start(row, inner) else {
            return false;
        };
        let aligned =
            next_start == start || self.after_marker(row, start, inner) == Some(next_start);
        aligned
            && (inner.start..next_start).all(|column| {
                self.blank(next, column)
                    || (self.cell(next, column).map(|cell| &cell.symbol)
                        == self.cell(row, column).map(|cell| &cell.symbol))
            })
    }

    /// Where the text after `row`'s first word starts, when that word is a
    /// marker: a short run without letters, such as `●`, `-`, or `12`.
    fn after_marker(&self, row: u16, start: u16, inner: &Range<u16>) -> Option<u16> {
        let first = self.word(row, start);
        let marker = first.end - first.start <= MAX_MARKER
            && first.clone().all(|column| {
                self.cell(row, column)
                    .is_some_and(|cell| !shown(cell).chars().any(char::is_alphabetic))
            });
        marker
            .then(|| (first.end..inner.end).find(|&column| self.content(row, column)))
            .flatten()
    }

    /// The block's wrap column: the furthest any text reaches between the
    /// same walls on the rows around `row`.
    fn wrap_column(&self, row: u16, walls: Walls, inner: &Range<u16>) -> u16 {
        let same = |other: &u16| {
            walls.left.is_none_or(|left| self.frame_at(*other, left))
                && walls.right.is_none_or(|right| self.frame_at(*other, right))
        };
        let rows = &self.bounds.rows;
        let above = (rows.start.max(row.saturating_sub(BLOCK_SCAN))..row)
            .rev()
            .take_while(same);
        let below = (row..rows.end.min(row.saturating_add(BLOCK_SCAN + 1))).take_while(same);
        above
            .chain(below)
            .filter_map(|other| self.content_end(other, inner))
            .max()
            .unwrap_or(inner.start)
    }

    /// Whether the word's walls are a table's: a neighbouring cell beyond
    /// either wall holds text on `row`.
    fn in_table(&self, row: u16, walls: Walls) -> bool {
        let open = |column: &u16| !self.frame_at(row, *column);
        let filled = |column: u16| self.content(row, column);
        let (start, end) = (self.bounds.columns.start, self.bounds.columns.end);
        let beyond_right = walls
            .right
            .is_some_and(|right| (right + 1..end).take_while(open).any(filled));
        let beyond_left = walls
            .left
            .is_some_and(|left| (start..left).rev().take_while(open).any(filled));
        beyond_right || beyond_left
    }

    /// Whether `row` holds no text outside `walls`.
    fn outside_blank(&self, row: u16, walls: Walls) -> bool {
        let (start, end) = (self.bounds.columns.start, self.bounds.columns.end);
        let left = start..walls.left.unwrap_or(start);
        let right = walls.right.map_or(end, |right| right + 1)..end;
        left.chain(right).all(|column| !self.content(row, column))
    }

    /// The cells of the whole word that starts with `word` on `row`, down
    /// to the first row where it stops before the wrap column.
    fn word_length(&self, row: u16, word: &Range<u16>, wrap: u16, inner: &Range<u16>) -> u16 {
        let mut length = word.end - word.start;
        let mut other = row;
        for _ in 0..MAX_ROWS {
            other = other.saturating_add(1);
            let Some(start) = self.content_start(other, inner) else {
                break;
            };
            let next = self.word(other, start);
            length = length.saturating_add(next.end - next.start);
            if next.end - 1 != wrap {
                break;
            }
        }
        length
    }

    /// Adds the rows below that continue `word`, the last piece's word.
    fn walk_down(&self, pieces: &mut Vec<Piece>, mut word: Range<u16>) -> Option<()> {
        while let Some(last) = pieces.last_mut()
            && let Some((join, start)) = self.join_down(last.row, &word)
        {
            last.columns.end = word.end;
            last.below = Some(join);
            let next = last.row + 1;
            pieces.push(Piece {
                row: next,
                columns: start..self.bounds.columns.end,
                below: None,
            });
            if pieces.len() > MAX_ROWS {
                return None;
            }
            word = self.word(next, start);
        }
        Some(())
    }

    /// Adds the rows above whose last word `word`, the first piece's,
    /// continues.
    fn walk_up(&self, pieces: &mut Vec<Piece>, mut word: Range<u16>) -> Option<()> {
        while let Some(first) = pieces.first()
            && let Some(above) = first.row.checked_sub(1)
            && self.bounds.rows.contains(&above)
            && let Some(end) =
                self.content_end(above, &self.inner(self.walls(first.row, &word, above)))
        {
            let candidate = self.word(above, end);
            match self.join_down(above, &candidate) {
                Some((join, start)) if start == word.start => {
                    pieces[0].columns.start = start;
                    pieces.insert(
                        0,
                        Piece {
                            row: above,
                            columns: self.bounds.columns.start..candidate.end,
                            below: Some(join),
                        },
                    );
                }
                _ => break,
            }
            if pieces.len() > MAX_ROWS {
                return None;
            }
            word = candidate;
        }
        Some(())
    }

    fn joined(&self, pieces: &[Piece]) -> Option<Joined> {
        let mut joined = Joined {
            text: String::new(),
            cells: Vec::new(),
            raw_joins: Vec::new(),
            ends_at_bottom: pieces
                .last()
                .is_some_and(|piece| piece.row + 1 == self.bounds.rows.end),
        };
        let mut above = None;
        for piece in pieces {
            if above == Some(Join::Raw) {
                joined.raw_joins.push(joined.text.len());
            }
            above = piece.below;
            let row_start = joined.text.len();
            for column in piece.columns.clone() {
                let cell = self.cell(piece.row, column)?;
                joined.cells.push(Placed {
                    byte: joined.text.len(),
                    row: piece.row,
                    column,
                    continuation: cell.skip,
                });
                if cell.skip {
                    continue;
                }
                let symbol = shown(cell);
                if joined.text.len() - row_start + symbol.len() > MAX_ROW_BYTES {
                    return None;
                }
                joined.text.push_str(symbol);
            }
        }
        (pieces.len() == 1 || joined.text.len() <= MAX_JOINED_BYTES).then_some(joined)
    }
}

/// Whether `wrap-ansi` breaks a word of `length` cells, too long for rows of
/// `width`, starting it in the `first` cells left on the current row rather
/// than on a row of its own: when that costs no extra row.
fn breaks_here(first: u16, length: u16, width: u16) -> bool {
    if width == 0 || first > width || length <= width {
        return false;
    }
    first == width || (length - 1) / width > (length - first - 1) / width
}

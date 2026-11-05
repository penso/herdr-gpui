//! Text selected in a herdr-core pane's visible screen: a drag from one cell
//! to another, a double-clicked word, or a triple-clicked line, read back as
//! the text a terminal would copy. Pure: cells in, ranges and text out.

use sheprd_backend::FrameData;
use std::ops::Range;

/// A cell of the visible screen, column and row from the top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Cell {
    pub(crate) row: u16,
    pub(crate) column: u16,
}

/// From where a press started to where the pointer is, either way round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    anchor: Cell,
    head: Cell,
    /// A double- or triple-clicked word or line, which selects its cells
    /// even when it is one cell wide.
    picked: bool,
}

impl Selection {
    /// A press at `at`, selecting nothing until the pointer moves.
    pub(crate) fn at(at: Cell) -> Self {
        Self {
            anchor: at,
            head: at,
            picked: false,
        }
    }

    /// The word under `at`: the run of non-blank cells around it.
    pub(crate) fn word(frame: &FrameData, at: Cell) -> Self {
        let blank = |column: u16| symbol(frame, at.row, column).trim().is_empty();
        if blank(at.column) {
            return Self::at(at);
        }
        let start = (0..at.column)
            .rev()
            .take_while(|column| !blank(*column))
            .last()
            .unwrap_or(at.column);
        let end = (at.column..frame.width)
            .take_while(|column| !blank(*column))
            .last()
            .unwrap_or(at.column);
        Self {
            anchor: Cell {
                row: at.row,
                column: start,
            },
            head: Cell {
                row: at.row,
                column: end,
            },
            picked: true,
        }
    }

    /// The whole row under `at`.
    pub(crate) fn line(frame: &FrameData, at: Cell) -> Self {
        Self {
            anchor: Cell {
                row: at.row,
                column: 0,
            },
            head: Cell {
                row: at.row,
                column: frame.width.saturating_sub(1),
            },
            picked: true,
        }
    }

    pub(crate) fn extend(&mut self, to: Cell) {
        self.head = to;
    }

    /// Whether this is a press whose pointer never left the cell it
    /// pressed, rather than a word or line, however short.
    pub(crate) fn is_empty(&self) -> bool {
        !self.picked && self.anchor == self.head
    }

    fn ordered(&self) -> (Cell, Cell) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Each selected row's columns, as a terminal selects: to the end of
    /// the first row, whole rows between, and the last row up to the head.
    pub(crate) fn rows(&self, width: u16) -> Vec<(u16, Range<u16>)> {
        if self.is_empty() || width == 0 {
            return Vec::new();
        }
        let (start, end) = self.ordered();
        (start.row..=end.row)
            .map(|row| {
                let from = if row == start.row { start.column } else { 0 };
                let to = if row == end.row {
                    end.column.saturating_add(1).min(width)
                } else {
                    width
                };
                (row, from.min(to)..to)
            })
            .collect()
    }

    /// The selected text: trailing blanks dropped, and a newline where a row
    /// ends. The frame has no soft-wrap flag, so a row that still has text in
    /// its last column is read as continuing onto the next row. A line that
    /// ends exactly on that column is joined the same way.
    pub(crate) fn text(&self, frame: &FrameData) -> String {
        let mut text = String::new();
        let mut continued = false;
        let mut any = false;
        for (row, columns) in self.rows(frame.width) {
            if row >= frame.height {
                continue;
            }
            if any && !continued {
                text.push('\n');
            }
            let end = columns.end;
            let line: String = columns.map(|column| symbol(frame, row, column)).collect();
            continued = end == frame.width && fills_edge(frame, row);
            text.push_str(line.trim_end());
            any = true;
        }
        text
    }
}

/// Whether `row` still has text in the frame's last column. Padding blanks
/// mean the line ended short of the edge.
fn fills_edge(frame: &FrameData, row: u16) -> bool {
    let column = frame.width.saturating_sub(1);
    let index = usize::from(row) * usize::from(frame.width) + usize::from(column);
    match frame.cells.get(index) {
        Some(cell) if cell.skip => true,
        Some(cell) => !cell.symbol.is_empty() && !cell.symbol.chars().all(char::is_whitespace),
        None => false,
    }
}

/// What a cell shows. A wide character's second cell shows nothing, so
/// the character is copied once.
fn symbol(frame: &FrameData, row: u16, column: u16) -> &str {
    let index = usize::from(row) * usize::from(frame.width) + usize::from(column);
    frame
        .cells
        .get(index)
        .filter(|cell| !cell.skip)
        .map_or("", |cell| cell.symbol.as_str())
}

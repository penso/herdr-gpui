//! Text selected in a herdr-core pane's visible screen: a drag from one cell
//! to another, a double-clicked word, or a triple-clicked line, read back as
//! the text a terminal would copy. Pure: cells in, ranges and text out.

use herdr_backend::FrameData;
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

    /// The selected text: each row's cells with trailing blanks dropped,
    /// rows joined by newlines.
    pub(crate) fn text(&self, frame: &FrameData) -> String {
        self.rows(frame.width)
            .into_iter()
            .filter(|(row, _)| *row < frame.height)
            .map(|(row, columns)| {
                let text: String = columns.map(|column| symbol(frame, row, column)).collect();
                text.trim_end().to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
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

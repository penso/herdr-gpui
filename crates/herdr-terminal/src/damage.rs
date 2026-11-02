//! What changed on screen since the last [`crate::Emulator::take_damage`].

use crate::{
    cells::{self, Links},
    listener::Listener,
};
use alacritty_terminal::{Term, term::TermDamage};
use herdr_protocol::{CursorState, FrameData, PaneSurfacePatchRow};

/// Changes since the previous take, ready to apply to the app's copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Damage {
    /// Everything may have changed: replace the whole frame. Its hyperlink
    /// table is the one later [`Damage::Rows`] index into.
    Full(FrameData),
    /// Only these whole rows changed (each starts at column 0), and the
    /// cursor is now here. Empty `rows` means only the cursor may have moved.
    Rows {
        rows: Vec<PaneSurfacePatchRow>,
        cursor: Option<CursorState>,
    },
}

impl Damage {
    /// Whether there is nothing to repaint.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Rows { rows, .. } if rows.is_empty())
    }
}

/// What the last take sent, so the next can tell what is new.
#[derive(Debug)]
pub(crate) struct Tracker {
    /// Set by anything alacritty's line damage does not describe: a new
    /// emulator, a resize, or scrolling the view.
    pub(crate) full: bool,
    /// Whether bytes arrived since the last take.
    pub(crate) fed: bool,
    alt_screen: bool,
    /// The hyperlink table of the last full frame sent.
    links: Vec<String>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            full: true,
            fed: false,
            alt_screen: false,
            links: Vec::new(),
        }
    }
}

impl Tracker {
    pub(crate) fn take(&mut self, term: &mut Term<Listener>, alt_screen: bool) -> Damage {
        let lines = match term.damage() {
            TermDamage::Full => None,
            TermDamage::Partial(lines) => Some(lines.map(|bounds| bounds.line).collect::<Vec<_>>()),
        };
        term.reset_damage();
        let fed = std::mem::take(&mut self.fed);
        let full = std::mem::take(&mut self.full) || alt_screen != self.alt_screen;
        self.alt_screen = alt_screen;
        let rows = match lines {
            Some(lines) if !full => {
                if !fed {
                    return Damage::Rows {
                        rows: Vec::new(),
                        cursor: cells::cursor(term),
                    };
                }
                self.rows(term, &lines)
            }
            _ => None,
        };
        match rows {
            Some(rows) => Damage::Rows {
                rows,
                cursor: cells::cursor(term),
            },
            None => Damage::Full(cells::frame(term, &mut self.links)),
        }
    }

    /// The damaged rows, or `None` when one links a URI the app has not seen.
    fn rows(&mut self, term: &Term<Listener>, lines: &[usize]) -> Option<Vec<PaneSurfacePatchRow>> {
        let mut links = Links::fixed(&mut self.links);
        let mut rows = Vec::with_capacity(lines.len());
        for &line in lines {
            rows.push(PaneSurfacePatchRow {
                x: 0,
                y: cells::side(line),
                cells: cells::row(term, line, &mut links)?,
            });
        }
        Some(rows)
    }
}

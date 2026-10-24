//! The pairing link as a QR code: the module matrix from the `qrcode` crate,
//! where it sits in a square, and the runs of dark modules the dialog paints
//! as quads. Pure, so the geometry is tested without a window.

use crate::app::{Error, Result};
use qrcode::{Color, EcLevel, QrCode};

/// Blank modules around the code, which scanners need to find its edges.
pub const QUIET_ZONE: usize = 4;

/// The code's modules, row by row; `true` is dark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qr {
    width: usize,
    dark: Vec<bool>,
    /// The dark modules as rows of runs, worked out once for painting.
    runs: Vec<Run>,
}

impl Qr {
    /// Medium error correction: a link of a hundred-odd characters stays a
    /// small, easily scanned code while a smudged screen still reads.
    pub fn encode(text: &str) -> Result<Self> {
        let code = QrCode::with_error_correction_level(text, EcLevel::M).map_err(Error::Qr)?;
        let mut qr = Self {
            width: code.width(),
            dark: code
                .to_colors()
                .into_iter()
                .map(|color| color == Color::Dark)
                .collect(),
            runs: Vec::new(),
        };
        qr.runs = qr.find_runs();
        Ok(qr)
    }

    /// Modules along a side.
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.width && self.dark.get(y * self.width + x).copied().unwrap_or(false)
    }

    /// Each row's dark modules joined into runs, so a code takes a few
    /// hundred quads rather than one per module.
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    fn find_runs(&self) -> Vec<Run> {
        let mut runs = Vec::new();
        for row in 0..self.width {
            let mut start = None;
            for column in 0..=self.width {
                match (start, self.is_dark(column, row)) {
                    (None, true) => start = Some(column),
                    (Some(first), false) => {
                        runs.push(Run {
                            row,
                            column: first,
                            length: column - first,
                        });
                        start = None;
                    }
                    _ => {}
                }
            }
        }
        runs
    }
}

/// Dark modules side by side in one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub row: usize,
    pub column: usize,
    pub length: usize,
}

/// Where a code sits in a square, in pixels from the square's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// One module's side: whole pixels, so every edge is crisp.
    pub module: f32,
    /// The first module's offset, past the margin and the quiet zone.
    pub origin: f32,
    /// The code with its quiet zone, which is painted light.
    pub size: f32,
    /// Where that light square starts.
    pub margin: f32,
}

/// The largest whole-pixel module that fits `modules` and the quiet zone in
/// a square of `side`, centred. `None` when not even one pixel a module fits.
pub fn layout(side: f32, modules: usize) -> Option<Layout> {
    if modules == 0 || !side.is_finite() {
        return None;
    }
    let cells = (modules + 2 * QUIET_ZONE) as f32;
    let module = (side / cells).floor();
    if module < 1. {
        return None;
    }
    let size = module * cells;
    let margin = ((side - size) / 2.).floor();
    Some(Layout {
        module,
        origin: margin + module * QUIET_ZONE as f32,
        size,
        margin,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

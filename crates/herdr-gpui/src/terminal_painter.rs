mod glyphs;
mod graphics;
mod images;

use self::glyphs::GlyphCache;
use self::graphics::Graphic;
use self::images::{ImageCache, ImageGeometry, below_text};
pub(crate) use self::images::{ImageTarget, PlacedImages};
use crate::config::Theme;
use crate::terminal::*;
use gpui::*;
use herdr_client::protocol::{CellData, FrameData, PaneSurfacePane, SurfaceRect};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthStr;

/// The selection tints the cells it covers instead of replacing their colors:
/// a terminal's own background is meaningful, and the glyphs above it stay
/// readable on every theme.
const SELECTION_ALPHA: u32 = 0x59;
/// Search matches tint like the selection, the current one strongly enough to
/// find at a glance among the others.
const MATCH_ALPHA: u32 = 0x4d;
const CURRENT_MATCH_ALPHA: u32 = 0xa6;
const REPORT_INTERVAL: Duration = Duration::from_secs(5);
const SLOW_PAINT: Duration = Duration::from_millis(16);
const SCROLLBAR_INSET: f32 = 1.;
const SCROLLBAR_ALPHA: u32 = 0xc0;

fn background_extent(
    grid: Size<Pixels>,
    available: Size<Pixels>,
    cell: Size<Pixels>,
) -> Size<Pixels> {
    let extend = |grid, available, cell| {
        if available > grid && available - grid < cell {
            available
        } else {
            grid
        }
    };
    size(
        extend(grid.width, available.width, cell.width),
        extend(grid.height, available.height, cell.height),
    )
}

/// Why a span of cells is tinted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tint {
    Selection,
    Match,
    CurrentMatch,
    /// The copy-mode cursor, drawn as a block over its cell.
    CopyCursor,
}

impl Tint {
    fn color(self, theme: &Theme) -> Rgba {
        match self {
            Self::Selection => rgba((theme.primary() << 8) | SELECTION_ALPHA),
            Self::Match => rgba((theme.palette[3] << 8) | MATCH_ALPHA),
            Self::CurrentMatch => rgba((theme.palette[3] << 8) | CURRENT_MATCH_ALPHA),
            Self::CopyCursor => rgba((theme.cursor << 8) | CURRENT_MATCH_ALPHA),
        }
    }
}

/// Tinted cells of one row, in the grid of the frame that paints them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Highlight {
    pub(crate) row: u16,
    pub(crate) columns: std::ops::Range<u16>,
    pub(crate) tint: Tint,
}

/// Cells of one row that a paint covers, in the grid of the frame it paints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) row: u16,
    pub(crate) columns: std::ops::Range<u16>,
}

/// Every cell of `frame`, row by row.
fn whole(frame: &FrameData) -> Vec<Span> {
    (0..frame.height)
        .map(|row| Span {
            row,
            columns: 0..frame.width,
        })
        .collect()
}

/// Whether `area` covers the cell at `x`, `y`.
pub(crate) fn covers(area: &[Span], x: u16, y: u16) -> bool {
    area.iter()
        .any(|span| span.row == y && span.columns.contains(&x))
}

/// The non-empty runs of `area` inside `frame`, as ranges of its cell indices.
/// A frame short of cells paints the ones it has.
pub(crate) fn cell_ranges<'a>(
    frame: &'a FrameData,
    area: &'a [Span],
) -> impl Iterator<Item = std::ops::Range<usize>> + 'a {
    let width = usize::from(frame.width);
    area.iter()
        .filter(|span| span.row < frame.height)
        .map(move |span| {
            let start = usize::from(span.row) * width;
            let end =
                |column: u16| (start + usize::from(column.min(frame.width))).min(frame.cells.len());
            end(span.columns.start)..end(span.columns.end)
        })
        .filter(|range| range.start < range.end)
}

/// The parts of `highlights` inside `area`.
pub(crate) fn clip(highlights: &[Highlight], area: &[Span]) -> Vec<Highlight> {
    highlights
        .iter()
        .flat_map(|highlight| {
            area.iter()
                .filter(move |span| span.row == highlight.row)
                .filter_map(move |span| {
                    let columns = highlight.columns.start.max(span.columns.start)
                        ..highlight.columns.end.min(span.columns.end);
                    (columns.start < columns.end).then_some(Highlight {
                        row: highlight.row,
                        columns,
                        tint: highlight.tint,
                    })
                })
        })
        .collect()
}

#[derive(Default)]
struct PaintTiming {
    count: u64,
    total: Duration,
    max: Duration,
    slow_count: u64,
}

struct PaintDiagnostics {
    since: Instant,
    timing: PaintTiming,
    last_error: Option<Instant>,
    errors: u64,
}

impl PaintDiagnostics {
    fn new(now: Instant) -> Self {
        Self {
            since: now,
            timing: PaintTiming::default(),
            last_error: None,
            errors: 0,
        }
    }

    fn record(&mut self, now: Instant, elapsed: Duration) -> Option<PaintTiming> {
        self.timing.count += 1;
        self.timing.total += elapsed;
        self.timing.max = self.timing.max.max(elapsed);
        self.timing.slow_count += u64::from(elapsed > SLOW_PAINT);
        if now.duration_since(self.since) < REPORT_INTERVAL {
            return None;
        }
        self.since = now;
        Some(std::mem::take(&mut self.timing))
    }

    fn take_errors(&mut self, now: Instant, errors: u64) -> Option<u64> {
        self.errors = self.errors.saturating_add(errors);
        if self.errors == 0
            || self
                .last_error
                .is_some_and(|last| now.duration_since(last) < REPORT_INTERVAL)
        {
            return None;
        }
        self.last_error = Some(now);
        Some(std::mem::take(&mut self.errors))
    }
}

pub(crate) struct TerminalPainter {
    font_size: f32,
    cell_height: f32,
    theme: Theme,
    config: Option<Font>,
    // Resolved foreground includes reverse, dim and hidden; only bold/italic
    // affect shaping. Decorations remain at exact cell-grid coordinates.
    glyphs: GlyphCache,
    cell_width: Option<f32>,
    diagnostics: PaintDiagnostics,
    images: ImageCache,
    /// Each image painted, with its z, in paint order.
    #[cfg(test)]
    painted_images: Vec<(i32, Bounds<Pixels>)>,
    #[cfg(feature = "integration-test")]
    pub uncached: bool,
}

impl Default for TerminalPainter {
    fn default() -> Self {
        Self {
            font_size: FONT_SIZE,
            cell_height: CELL_HEIGHT,
            theme: Theme::default(),
            config: None,
            glyphs: GlyphCache::default(),
            cell_width: None,
            diagnostics: PaintDiagnostics::new(Instant::now()),
            images: ImageCache::default(),
            #[cfg(test)]
            painted_images: Vec::new(),
            #[cfg(feature = "integration-test")]
            uncached: false,
        }
    }
}

/// The cell rectangle covering every in-frame cell of `rows`, if any.
fn link_bounds(frame: &FrameData, rows: &[(u16, std::ops::Range<u16>)]) -> Option<SurfaceRect> {
    let mut cells = rows.iter().filter_map(|(row, columns)| {
        let end = columns.end.min(frame.width);
        (*row < frame.height && columns.start < end).then_some((*row, columns.start, end))
    });
    let (row, start, end) = cells.next()?;
    let (top, bottom, left, right) = cells.fold(
        (row, row, start, end),
        |(top, bottom, left, right), (row, start, end)| {
            (
                top.min(row),
                bottom.max(row),
                left.min(start),
                right.max(end),
            )
        },
    );
    Some(SurfaceRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top + 1,
    })
}

fn decoration_offsets(cell: &CellData, cell_height: f32) -> impl Iterator<Item = f32> + '_ {
    [
        (UNDERLINE, cell_height - 2.),
        (STRIKETHROUGH, cell_height / 2.),
    ]
    .into_iter()
    .filter_map(|(modifier, y)| (cell.modifier & modifier != 0).then_some(y))
}

/// `ShapedLine::paint` without its per-call layer, whose BoundsTree insert
/// would otherwise run once per cell. Glyph placement matches GPUI's.
fn paint_glyphs(
    line: &ShapedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
    color: Rgba,
    window: &mut Window,
) -> Result<()> {
    let baseline = origin
        + point(
            px(0.),
            (line_height - line.ascent - line.descent) / 2. + line.ascent,
        );
    for run in &line.runs {
        for glyph in &run.glyphs {
            let position = baseline + point(glyph.position.x, px(0.));
            if glyph.is_emoji {
                window.paint_emoji(position, run.font_id, glyph.id, line.font_size)?;
            } else {
                window.paint_glyph(
                    position,
                    run.font_id,
                    glyph.id,
                    line.font_size,
                    color.into(),
                )?;
            }
        }
    }
    Ok(())
}

fn background_spans<'a>(
    row: &'a [CellData],
    columns: std::ops::Range<usize>,
    theme: &'a Theme,
) -> impl Iterator<Item = (usize, usize, u32)> + 'a {
    // A wide glyph's continuation cell shows the glyph's background, as a host
    // terminal does: Herdr's ANSI renderer never draws that cell, so its own
    // background is not meant to be seen. Like that renderer, a halfwidth
    // katakana with a (semi-)voiced mark counts as two columns.
    let wide = |symbol: &str| {
        let mut chars = symbol.chars();
        symbol.width() > 1
            || matches!(
                (chars.next(), chars.next(), chars.next()),
                (
                    Some('\u{ff66}'..='\u{ff9d}'),
                    Some('\u{ff9e}' | '\u{ff9f}'),
                    None
                )
            )
    };
    let bg = move |x: usize| {
        let x = if x > 0 && wide(&row[x - 1].symbol) {
            x - 1
        } else {
            x
        };
        cell_colors(&row[x], theme).1
    };
    let mut start = columns.start;
    let stop = columns.end.min(row.len());
    std::iter::from_fn(move || {
        (start < stop).then_some(())?;
        let color = bg(start);
        let mut end = start + 1;
        while end < stop && bg(end) == color {
            end += 1;
        }
        let span = (start, end, color);
        start = end;
        Some(span)
    })
}

/// Where an IME composition sits: at the input cursor, shifted left only as
/// far as it takes to end inside the grid. Text wider than the grid loses its
/// start rather than its end, where the IME is editing.
fn composition_origin(cursor: Point<Pixels>, width: Pixels, grid: Bounds<Pixels>) -> Point<Pixels> {
    point(cursor.x.min(grid.right() - width), cursor.y)
}

/// The byte index of a UTF-16 offset, as the platform input handler counts.
fn byte_index(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    text.char_indices()
        .find(|(_, c)| {
            let found = units >= utf16;
            units += c.len_utf16();
            found
        })
        .map_or(text.len(), |(index, _)| index)
}

impl TerminalPainter {
    pub fn set_appearance(&mut self, font_size: f32, cell_height: f32, theme: Theme) {
        if self.font_size != font_size || self.cell_height != cell_height || self.theme != theme {
            self.font_size = font_size;
            self.cell_height = cell_height;
            self.theme = theme;
            self.glyphs.clear();
            self.cell_width = None;
        }
    }

    #[cfg(feature = "integration-test")]
    pub fn reset_cache(&mut self) {
        self.config = None;
        self.glyphs.clear();
        self.cell_width = None;
    }

    #[cfg(feature = "integration-test")]
    pub fn verify_native_cache(&self, window: &Window) -> Result<usize> {
        let Some(base) = &self.config else {
            anyhow::bail!("missing font config");
        };
        for (style, symbol, cached) in self.glyphs.iter() {
            let fresh = self.shape(base, style, &symbol, window);
            // Includes native glyph IDs/positions, font IDs and metrics.
            if format!("{fresh:?}") != format!("{cached:?}") {
                anyhow::bail!("cached glyph/style mismatch: {symbol:?}");
            }
        }
        Ok(self.glyphs.len())
    }

    /// Glyphs for one cell symbol. Color is left to `paint_glyphs`, so one
    /// shape serves every color the symbol is drawn in.
    fn shape(&self, font: &Font, style: usize, symbol: &str, window: &Window) -> ShapedLine {
        let mut font = font.clone();
        let modifier = glyphs::style_modifier(style);
        if modifier & BOLD != 0 {
            font.weight = FontWeight::BOLD;
        }
        if modifier & ITALIC != 0 {
            font.style = FontStyle::Italic;
        }
        window.text_system().shape_line(
            SharedString::from(symbol.to_owned()),
            px(self.font_size),
            &[TextRun {
                len: symbol.len(),
                font,
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    }

    fn configure(&mut self, font: &Font) {
        if self.config.as_ref() != Some(font) {
            self.glyphs.clear();
            self.cell_width = None;
            self.config = Some(font.clone());
        }
    }

    pub fn cell_width(&mut self, font: &Font, window: &Window, cx: &mut App) -> f32 {
        self.configure(font);
        if let Some(width) = self.cell_width {
            return width;
        }
        let width = window
            .text_system()
            .shape_line(
                "M".into(),
                px(self.font_size),
                &[TextRun {
                    len: 1,
                    font: font.clone(),
                    color: rgb(self.theme.foreground).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
            .to_f64() as f32;
        #[cfg(feature = "integration-test")]
        {
            cx.default_global::<crate::performance::Counts>()
                .metric_shapes += 1;
        }
        #[cfg(not(feature = "integration-test"))]
        let _ = cx;
        self.cell_width = Some(width);
        width
    }

    /// Underlines the cells of a hovered link, painted after `paint_frame`
    /// for the same frame and origin. Each cell takes its own text color, as
    /// an SGR underline would, and spaces along the link are underlined too.
    pub fn paint_link(
        &self,
        frame: &FrameData,
        origin: Point<Pixels>,
        cell_width: f32,
        rows: &[(u16, std::ops::Range<u16>)],
        window: &mut Window,
    ) {
        let Some(bounds) = link_bounds(frame, rows) else {
            return;
        };
        let layer = Bounds::new(
            origin
                + point(
                    px(f32::from(bounds.x) * cell_width),
                    px(f32::from(bounds.y) * self.cell_height),
                ),
            size(
                px(f32::from(bounds.width) * cell_width),
                px(f32::from(bounds.height) * self.cell_height),
            ),
        );
        window.paint_layer(layer, |window| {
            for (row, columns) in rows {
                if *row >= frame.height {
                    continue;
                }
                for column in columns.start..columns.end.min(frame.width) {
                    let index = usize::from(*row) * usize::from(frame.width) + usize::from(column);
                    let Some(data) = frame.cells.get(index) else {
                        continue;
                    };
                    let position = origin
                        + point(
                            px(f32::from(column) * cell_width),
                            px(f32::from(*row) * self.cell_height + self.cell_height - 2.),
                        );
                    window.paint_quad(fill(
                        Bounds::new(position, size(px(cell_width), px(1.))),
                        rgb(cell_colors(data, &self.theme).0),
                    ));
                }
            }
        });
    }

    /// Paints the cells of one frame that `area` covers, or all of them,
    /// tinting the cells `highlights` name in that frame's own grid. Rows
    /// outside the frame are ignored: a selection or search was made against
    /// the live surface, which a repaint may already have replaced.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_frame(
        &mut self,
        frame: &FrameData,
        origin: Point<Pixels>,
        available: Option<Size<Pixels>>,
        cell_width: f32,
        font: &Font,
        highlights: &[Highlight],
        panes: &[PaneSurfacePane],
        area: Option<&[Span]>,
        images: Option<PlacedImages<'_>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if frame.width == 0 {
            return;
        }
        // CPU scene construction only: this does not measure GPU completion.
        let started = Instant::now();
        let mut paint_errors = 0_u64;
        self.configure(font);
        let cached = true;
        #[cfg(feature = "integration-test")]
        let cached = cached && !self.uncached;
        #[cfg(feature = "integration-test")]
        let mut counts = crate::performance::Counts::default();
        let grid = Bounds::new(
            origin,
            size(
                px(f32::from(frame.width) * cell_width),
                px(f32::from(frame.height) * self.cell_height),
            ),
        );
        // Only fill a sub-cell remainder. Retained frames during resize and
        // mirrored groups must not stretch across whole missing rows/columns.
        // Popups have no remainder; their background stays inside their grid.
        let background = available.map_or(grid.size, |available| {
            background_extent(
                grid.size,
                available,
                size(px(cell_width), px(self.cell_height)),
            )
        });
        let whole_area;
        let area = match area {
            Some(area) => area,
            None => {
                whole_area = whole(frame);
                &whole_area
            }
        };
        // The daemon's cell scrollbar is replaced by the pixel thumb painted below.
        let bars: Vec<SurfaceRect> = panes.iter().filter_map(|p| p.scrollbar_rect).collect();
        let in_bar = |index: usize| {
            let width = usize::from(frame.width);
            bars.iter()
                .any(|r| in_rect(*r, (index % width) as u16, (index / width) as u16))
        };
        let placed = images.map_or_else(Vec::new, |images| {
            self.images.prepare(
                images,
                origin,
                (cell_width, self.cell_height),
                grid,
                window,
                cx,
            )
        });
        let (below, above): (Vec<_>, Vec<_>) =
            placed.into_iter().partition(|(z, ..)| below_text(*z));
        // A layer gives all its primitives one draw order, skipping GPUI's
        // per-primitive BoundsTree insert that dominates large grids. Within a
        // layer quads draw before glyphs, so decorations and the cursor take a
        // second layer above the text.
        window.paint_layer(Bounds::new(origin, background), |window| {
            // Backgrounds precede all glyphs, including wide graphemes' skip cells.
            let width = usize::from(frame.width);
            for range in cell_ranges(frame, area) {
                let y = range.start / width;
                let row = &frame.cells[y * width..((y + 1) * width).min(frame.cells.len())];
                let columns = range.start - y * width..range.end - y * width;
                let mut paint = |start: usize, end: usize, color| {
                    let right = if end == usize::from(frame.width) {
                        background.width
                    } else {
                        px(end as f32 * cell_width)
                    };
                    let bottom = if y + 1 == usize::from(frame.height) {
                        background.height
                    } else {
                        px((y + 1) as f32 * self.cell_height)
                    };
                    window.paint_quad(fill(
                        Bounds::new(
                            origin
                                + point(
                                    px(start as f32 * cell_width),
                                    px(y as f32 * self.cell_height),
                                ),
                            size(
                                right - px(start as f32 * cell_width),
                                bottom - px(y as f32 * self.cell_height),
                            ),
                        ),
                        rgb(color),
                    ));
                    #[cfg(feature = "integration-test")]
                    {
                        counts.quads += 1;
                    }
                };
                if cached {
                    for (start, end, color) in background_spans(row, columns, &self.theme) {
                        paint(start, end, color);
                    }
                } else {
                    for x in columns {
                        paint(x, x + 1, cell_colors(&row[x], &self.theme).1);
                    }
                }
            }
            // Between the backgrounds and the glyphs, so the tint reads as chosen
            // without hiding either.
            for Highlight { row, columns, tint } in &clip(highlights, area) {
                let (start, end) = (columns.start.min(frame.width), columns.end.min(frame.width));
                if *row >= frame.height || start >= end {
                    continue;
                }
                window.paint_quad(fill(
                    Bounds::new(
                        origin
                            + point(
                                px(f32::from(start) * cell_width),
                                px(f32::from(*row) * self.cell_height),
                            ),
                        size(
                            px(f32::from(end - start) * cell_width),
                            px(self.cell_height),
                        ),
                    ),
                    tint.color(&self.theme),
                ));
                #[cfg(feature = "integration-test")]
                {
                    counts.quads += 1;
                }
            }
            if below.is_empty() {
                paint_errors += self.paint_text(
                    frame,
                    area,
                    origin,
                    cell_width,
                    font,
                    cached,
                    &in_bar,
                    window,
                    #[cfg(feature = "integration-test")]
                    &mut counts,
                );
            }
        });
        if !below.is_empty() {
            paint_errors += self.paint_images(&below, window);
            window.paint_layer(grid, |window| {
                paint_errors += self.paint_text(
                    frame,
                    area,
                    origin,
                    cell_width,
                    font,
                    cached,
                    &in_bar,
                    window,
                    #[cfg(feature = "integration-test")]
                    &mut counts,
                );
            });
        }
        window.paint_layer(grid, |window| {
            // Box and block graphics are quads, so they share this layer to stay
            // above the backgrounds. Decorations cover the grid, including spaces
            // and wide-glyph continuation cells.
            for index in cell_ranges(frame, area).flatten() {
                let cell = &frame.cells[index];
                if in_bar(index) {
                    continue;
                }
                let position = origin
                    + point(
                        px((index % usize::from(frame.width)) as f32 * cell_width),
                        px((index / usize::from(frame.width)) as f32 * self.cell_height),
                    );
                if let Some(graphic) = (!cell.skip)
                    .then(|| Graphic::from_symbol(&cell.symbol))
                    .flatten()
                {
                    let color = rgb(cell_colors(cell, &self.theme).0);
                    graphic.rectangles(
                        Bounds::new(position, size(px(cell_width), px(self.cell_height))),
                        window.scale_factor(),
                        |bounds| {
                            window.paint_quad(fill(bounds, color));
                            #[cfg(feature = "integration-test")]
                            {
                                counts.quads += 1;
                            }
                        },
                    );
                }
                for y in decoration_offsets(cell, self.cell_height) {
                    window.paint_quad(fill(
                        Bounds::new(
                            position + point(px(0.), px(y)),
                            size(px(cell_width), px(1.)),
                        ),
                        rgb(cell_colors(cell, &self.theme).0),
                    ));
                    #[cfg(feature = "integration-test")]
                    {
                        counts.decorations += 1;
                    }
                }
            }
            if let Some(cursor) = frame.cursor.as_ref().filter(|c| {
                c.visible && c.x < frame.width && c.y < frame.height && covers(area, c.x, c.y)
            }) {
                let position = origin + cursor_offset(cursor, cell_width, self.cell_height);
                let (offset, dimensions) = match cursor.shape {
                    3 | 4 => (
                        point(px(0.), px(self.cell_height - 2.)),
                        size(px(cell_width), px(2.)),
                    ),
                    5 | 6 => (point(px(0.), px(0.)), size(px(2.), px(self.cell_height))),
                    _ => (
                        point(px(0.), px(0.)),
                        size(px(cell_width), px(self.cell_height)),
                    ),
                };
                window.paint_quad(fill(
                    Bounds::new(position + offset, dimensions),
                    rgba((self.theme.cursor << 8) | 0x80),
                ));
                #[cfg(feature = "integration-test")]
                {
                    counts.decorations += 1;
                }
            }
            // A scrollbar paints with the cells at its top, so one paint owns it.
            for bar in panes
                .iter()
                .filter(|pane| {
                    pane.scrollbar_rect
                        .is_some_and(|rect| covers(area, rect.x, rect.y))
                })
                .filter_map(|pane| Scrollbar::new(pane, cell_width, self.cell_height))
            {
                let width = (f32::from(bar.track.size.width) - 2. * SCROLLBAR_INSET).clamp(2., 6.);
                window.paint_quad(
                    fill(
                        Bounds::new(
                            origin
                                + point(
                                    bar.track.right() - px(width + SCROLLBAR_INSET),
                                    bar.thumb.top(),
                                ),
                            size(px(width), bar.thumb.size.height),
                        ),
                        rgba((self.theme.muted << 8) | SCROLLBAR_ALPHA),
                    )
                    .corner_radii(px(width / 2.)),
                );
            }
        });
        // Kitty draws `z >= 0` over the text, decorations and cursor included.
        paint_errors += self.paint_images(&above, window);
        #[cfg(feature = "integration-test")]
        {
            let total = cx.default_global::<crate::performance::Counts>();
            total.shapes += counts.shapes;
            total.quads += counts.quads;
            total.glyphs += counts.glyphs;
            total.decorations += counts.decorations;
            total.paint_errors += counts.paint_errors;
            total.paints += 1;
        }
        #[cfg(not(feature = "integration-test"))]
        let _ = cx;
        let now = Instant::now();
        if let Some(timing) = self.diagnostics.record(now, now.duration_since(started)) {
            let mean_ms = timing.total.as_secs_f64() * 1000. / timing.count as f64;
            let max_ms = timing.max.as_secs_f64() * 1000.;
            if timing.slow_count > 0 {
                tracing::warn!(
                    count = timing.count,
                    mean_ms,
                    max_ms,
                    slow_count = timing.slow_count,
                    "Terminal CPU paint timing"
                );
            } else {
                tracing::debug!(
                    count = timing.count,
                    mean_ms,
                    max_ms,
                    slow_count = timing.slow_count,
                    "Terminal CPU paint timing"
                );
            }
        }
        if let Some(count) = self.diagnostics.take_errors(now, paint_errors) {
            tracing::warn!(category = "glyph_paint", count, "Terminal paint failed");
        }
    }

    /// Paints images in order, returning how many failed.
    fn paint_images(
        &mut self,
        placed: &[(i32, ImageGeometry, Arc<RenderImage>)],
        window: &mut Window,
    ) -> u64 {
        let mut failed = 0;
        for (_z, geometry, texture) in placed {
            let painted = window.paint_image(
                geometry.visible,
                geometry.image,
                Corners::default(),
                texture.clone(),
                0,
                false,
            );
            failed += u64::from(painted.is_err());
            #[cfg(test)]
            self.painted_images.push((*_z, geometry.visible));
        }
        failed
    }

    /// Shapes and paints every visible glyph `area` covers, returning failures.
    #[allow(clippy::too_many_arguments)]
    fn paint_text(
        &mut self,
        frame: &FrameData,
        area: &[Span],
        origin: Point<Pixels>,
        cell_width: f32,
        font: &Font,
        cached: bool,
        in_bar: &impl Fn(usize) -> bool,
        window: &mut Window,
        #[cfg(feature = "integration-test")] counts: &mut crate::performance::Counts,
    ) -> u64 {
        let mut paint_errors = 0;
        for index in cell_ranges(frame, area).flatten() {
            let cell = &frame.cells[index];
            if cell.skip
                || cell.symbol.is_empty()
                || cell.symbol == " "
                || in_bar(index)
                || Graphic::from_symbol(&cell.symbol).is_some()
            {
                continue;
            }
            let style = glyphs::style(cell.modifier);
            let newly_shaped;
            let shaped = match cached
                .then(|| self.glyphs.get(style, &cell.symbol))
                .flatten()
            {
                Some(line) => line,
                None => {
                    #[cfg(feature = "integration-test")]
                    {
                        counts.shapes += 1;
                    }
                    let line = self.shape(font, style, &cell.symbol, window);
                    if cached && self.glyphs.has_room() {
                        self.glyphs.insert(style, &cell.symbol, line)
                    } else {
                        newly_shaped = line;
                        &newly_shaped
                    }
                }
            };
            let position = origin
                + point(
                    px((index % usize::from(frame.width)) as f32 * cell_width),
                    px((index / usize::from(frame.width)) as f32 * self.cell_height),
                );
            let color = rgb(cell_colors(cell, &self.theme).0);
            let result = paint_glyphs(shaped, position, px(self.cell_height), color, window);
            paint_errors += u64::from(result.is_err());
            #[cfg(feature = "integration-test")]
            {
                counts.glyphs += shaped.runs.iter().map(|r| r.glyphs.len()).sum::<usize>();
                counts.paint_errors += usize::from(result.is_err());
            }
        }
        paint_errors
    }

    /// Paints an uncommitted IME composition over the cells at the input
    /// cursor. It is shaped as one line rather than per cell, so it can span
    /// wide glyphs, and it bypasses the glyph cache since it changes with
    /// every keystroke. Nothing reaches the pane until the IME commits.
    pub fn paint_composition(
        &self,
        text: &str,
        cursor: Point<Pixels>,
        grid: Bounds<Pixels>,
        font: &Font,
        window: &mut Window,
    ) {
        if text.is_empty() {
            return;
        }
        let line = self.shape(font, 0, text, window);
        let origin = composition_origin(cursor, line.width, grid);
        let bounds = Bounds::new(origin, size(line.width, px(self.cell_height)));
        let color = rgb(self.theme.foreground);
        // A layer of its own, painted after the frame, keeps it above the
        // cells and the cursor it covers. It clips to the input area, so text
        // wider than a popup never paints over the pane beneath it.
        window.with_content_mask(Some(ContentMask { bounds: grid }), |window| {
            window.paint_layer(bounds, |window| {
                window.paint_quad(fill(bounds, rgb(self.theme.background)));
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(px(0.), px(self.cell_height - 2.)),
                        size(line.width, px(1.)),
                    ),
                    color,
                ));
                if paint_glyphs(&line, origin, px(self.cell_height), color, window).is_err() {
                    tracing::warn!(category = "glyph_paint", "IME composition paint failed");
                }
            });
        });
    }

    /// The bounds of `range` (UTF-16) within a composition painted by
    /// `paint_composition`, so the IME can place its candidate window under
    /// the clause being converted.
    pub fn composition_bounds(
        &self,
        text: &str,
        range: std::ops::Range<usize>,
        cursor: Bounds<Pixels>,
        grid: Bounds<Pixels>,
        font: &Font,
        window: &Window,
    ) -> Bounds<Pixels> {
        if text.is_empty() {
            return cursor;
        }
        let line = self.shape(font, 0, text, window);
        let origin = composition_origin(cursor.origin, line.width, grid);
        let start = line.x_for_index(byte_index(text, range.start));
        let end = line.x_for_index(byte_index(text, range.end));
        let width = (end - start).max(cursor.size.width);
        // A caret after the text, or a clause scrolled off the left, still
        // anchors the candidate window inside the grid.
        let x = (origin.x + start)
            .min(grid.right() - width)
            .max(grid.left());
        Bounds::new(point(x, origin.y), size(width, cursor.size.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn link_underlines_stay_inside_the_frame() {
        let frame = FrameData {
            width: 10,
            height: 3,
            cells: vec![cell("a"); 30],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        let rect = |x, y, width, height| SurfaceRect {
            x,
            y,
            width,
            height,
        };
        // A wrapped link's two rows, the second past the frame's right edge.
        assert_eq!(
            link_bounds(&frame, &[(1, 4..10), (2, 0..14)]),
            Some(rect(0, 1, 10, 2))
        );
        // Rows a resize has already removed, and empty ranges, paint nothing.
        assert_eq!(link_bounds(&frame, &[(3, 0..4), (0, 5..5)]), None);
        assert_eq!(link_bounds(&frame, &[(0, 12..14)]), None);
        assert_eq!(link_bounds(&frame, &[]), None);
    }

    #[gpui::test]
    fn edge_backgrounds_reach_the_canvas_without_stretching_popups(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        for extend in [false, true] {
            cx.draw(Point::default(), size(px(100.), px(100.)), |_, _| {
                canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        let frame = FrameData {
                            width: 2,
                            height: 2,
                            cells: [0x123456, 0x654321, 0xabcdef, 0xfedcba]
                                .into_iter()
                                .map(|bg| CellData {
                                    bg: 0x02000000 | bg,
                                    ..cell(" ")
                                })
                                .collect(),
                            cursor: None,
                            hyperlinks: vec![],
                            graphics: vec![],
                        };
                        let mut painter = TerminalPainter::default();
                        painter.set_appearance(14., 20., Theme::default());
                        painter.paint_frame(
                            &frame,
                            point(px(17.), px(23.)),
                            extend.then(|| size(px(23.), px(47.))),
                            10.,
                            &font("Menlo"),
                            &[],
                            &[],
                            None,
                            None,
                            window,
                            cx,
                        );
                    },
                )
                .size_full()
            });
            cx.update(|window, _| {
                let quads = window.painted_quads();
                assert_eq!(quads.len(), 4);
                for (x, y, width, height, color) in [
                    (17., 23., 10., 20., 0x123456),
                    (27., 23., if extend { 13. } else { 10. }, 20., 0x654321),
                    (17., 43., 10., if extend { 27. } else { 20. }, 0xabcdef),
                    (
                        27.,
                        43.,
                        if extend { 13. } else { 10. },
                        if extend { 27. } else { 20. },
                        0xfedcba,
                    ),
                ] {
                    let bounds = Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
                        .scale(window.scale_factor());
                    assert!(
                        quads
                            .iter()
                            .any(|quad| quad.bounds == bounds
                                && quad.background == rgb(color).into())
                    );
                }
            });
        }
    }

    #[test]
    fn backgrounds_fill_only_fractional_cell_remainders() {
        let cell = size(px(10.), px(20.));
        let available = size(px(103.), px(67.));
        let viewport = viewport(103., 67., 10., 20.);
        let grid = size(
            px(f32::from(viewport.cols) * 10.),
            px(f32::from(viewport.rows) * 20.),
        );
        assert_eq!(grid, size(px(100.), px(60.)));
        assert_eq!(background_extent(grid, available, cell), available);
        for (available, expected) in [
            (size(px(100.), px(60.)), grid),
            (size(px(99.), px(59.)), grid),
            (size(px(110.), px(80.)), grid),
            (size(px(111.), px(67.)), size(px(100.), px(67.))),
            (size(px(103.), px(81.)), size(px(103.), px(60.))),
        ] {
            assert_eq!(background_extent(grid, available, cell), expected);
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn paint_timing_threshold_interval_and_reset() {
        let start = Instant::now();
        let mut diagnostics = PaintDiagnostics::new(start);
        assert!(diagnostics.record(start, SLOW_PAINT).is_none());
        assert!(
            diagnostics
                .record(
                    start + REPORT_INTERVAL - Duration::from_nanos(1),
                    Duration::from_millis(17)
                )
                .is_none()
        );
        let timing = diagnostics
            .record(start + REPORT_INTERVAL, Duration::from_millis(3))
            .unwrap();
        assert_eq!(timing.count, 3);
        assert_eq!(timing.total, Duration::from_millis(36));
        assert_eq!(timing.max, Duration::from_millis(17));
        assert_eq!(timing.slow_count, 1);
        let timing = diagnostics
            .record(start + REPORT_INTERVAL * 2, Duration::from_millis(2))
            .unwrap();
        assert_eq!(timing.count, 1);
        assert_eq!(timing.total, Duration::from_millis(2));
        assert_eq!(timing.max, Duration::from_millis(2));
        assert_eq!(timing.slow_count, 0);
    }

    #[test]
    fn paint_errors_report_immediately_then_coalesce() {
        let start = Instant::now();
        let mut diagnostics = PaintDiagnostics::new(start);
        assert_eq!(diagnostics.take_errors(start, 0), None);
        assert_eq!(diagnostics.take_errors(start, 2), Some(2));
        assert_eq!(diagnostics.take_errors(start, 3), None);
        assert_eq!(
            diagnostics.take_errors(start + REPORT_INTERVAL - Duration::from_nanos(1), 4),
            None
        );
        assert_eq!(diagnostics.take_errors(start + REPORT_INTERVAL, 0), Some(7));
        assert_eq!(
            diagnostics.take_errors(start + REPORT_INTERVAL * 2, 0),
            None
        );
    }

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

    #[test]
    fn decorations_cover_spaces_empty_and_wide_continuation_cells() {
        for (symbol, skip) in [("x", false), (" ", false), ("", false), ("", true)] {
            let mut cell = CellData {
                skip,
                ..cell(symbol)
            };
            assert_eq!(decoration_offsets(&cell, CELL_HEIGHT).count(), 0);
            cell.modifier = UNDERLINE;
            assert_eq!(
                decoration_offsets(&cell, CELL_HEIGHT).collect::<Vec<_>>(),
                vec![18.]
            );
            cell.modifier = STRIKETHROUGH;
            assert_eq!(
                decoration_offsets(&cell, CELL_HEIGHT).collect::<Vec<_>>(),
                vec![10.]
            );
            cell.modifier = UNDERLINE | STRIKETHROUGH;
            assert_eq!(
                decoration_offsets(&cell, CELL_HEIGHT).collect::<Vec<_>>(),
                vec![18., 10.]
            );
            assert_eq!(
                decoration_offsets(&cell, 30.5).collect::<Vec<_>>(),
                vec![28.5, 15.25]
            );
        }
    }

    #[cfg(feature = "integration-test")]
    #[gpui::test]
    fn blank_cells_paint_decorations_without_shaping(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                |bounds, _, window, cx| {
                    let frame = FrameData {
                        width: 3,
                        height: 1,
                        cells: [(" ", false), ("", false), ("", true)]
                            .into_iter()
                            .map(|(symbol, skip)| CellData {
                                modifier: UNDERLINE | STRIKETHROUGH,
                                skip,
                                ..cell(symbol)
                            })
                            .collect(),
                        cursor: None,
                        hyperlinks: vec![],
                        graphics: vec![],
                    };
                    let mut painter = TerminalPainter::default();
                    for (font_size, cell_height, theme) in [
                        (FONT_SIZE, CELL_HEIGHT, Theme::default()),
                        (
                            21.35,
                            30.5,
                            Theme {
                                foreground: 0xabcdef,
                                background: 0x123456,
                                ..Theme::default()
                            },
                        ),
                    ] {
                        painter.set_appearance(font_size, cell_height, theme);
                        let before = cx
                            .default_global::<crate::performance::Counts>()
                            .decorations;
                        painter.paint_frame(
                            &frame,
                            bounds.origin,
                            None,
                            8.5,
                            &font("Menlo"),
                            &[],
                            &[],
                            None,
                            None,
                            window,
                            cx,
                        );
                        assert_eq!(painter.glyphs.len(), 0);
                        assert_eq!(
                            cx.default_global::<crate::performance::Counts>()
                                .decorations
                                - before,
                            6
                        );
                    }
                },
            )
            .size_full()
        });
    }

    #[test]
    fn spans_cover_skip_cells_and_resolved_colors_without_crossing_rows() {
        let theme = Theme::default();
        let mut row = vec![cell("\u{754c}"), cell(""), cell("x"), cell("x")];
        row[1].skip = true;
        row[2].fg = 0x02123456;
        row[2].modifier = 64;
        row[3].bg = 0x02123456;
        assert_eq!(
            background_spans(&row, 0..row.len(), &theme).collect::<Vec<_>>(),
            vec![(0, 2, BACKGROUND), (2, 4, 0x123456)]
        );
        assert_eq!(background_spans(&[], 0..0, &theme).count(), 0);
        for cells in row.chunks(2) {
            let expanded: Vec<_> = background_spans(cells, 0..cells.len(), &theme)
                .flat_map(|(a, b, color)| (a..b).map(move |_| color))
                .collect();
            assert_eq!(
                expanded,
                cells
                    .iter()
                    .map(|c| cell_colors(c, &theme).1)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn wide_continuation_cells_take_the_glyph_background() {
        let theme = Theme::default();
        // The daemon sends continuation cells with a background of their own.
        let mut row = vec![cell("\u{3053}"), cell(""), cell("x"), cell("")];
        row[0].bg = 0x02373737;
        row[1].bg = 0x02000000;
        row[3].bg = 0x02000000;
        assert_eq!(
            background_spans(&row, 0..row.len(), &theme).collect::<Vec<_>>(),
            vec![(0, 2, 0x373737), (2, 3, BACKGROUND), (3, 4, 0)]
        );
        // Halfwidth katakana with a voiced or semi-voiced mark is two columns
        // wide in Herdr although its Unicode width is one.
        for kana in ["\u{ff76}\u{ff9e}", "\u{ff8a}\u{ff9f}"] {
            let mut row = vec![cell(kana), cell(""), cell("\u{ff76}"), cell("")];
            row[0].bg = 0x02373737;
            row[1].bg = 0x02000000;
            row[3].bg = 0x02000000;
            assert_eq!(
                background_spans(&row, 0..row.len(), &theme).collect::<Vec<_>>(),
                vec![(0, 2, 0x373737), (2, 3, BACKGROUND), (3, 4, 0)],
                "{kana}"
            );
        }
    }

    #[test]
    fn cache_style_is_the_font_face_alone() {
        // Color, dim, reverse, hidden and grid decorations are painted, not shaped.
        for modifier in [0, 2, 8, 64, 128, 256, 2 | 64 | 128 | 256] {
            assert_eq!(glyphs::style(modifier), 0, "{modifier}");
        }
        let styles = [BOLD, ITALIC, BOLD | ITALIC].map(glyphs::style);
        assert_eq!(styles, [1, 2, 3]);
        for style in 0..4 {
            assert_eq!(glyphs::style(glyphs::style_modifier(style)), style);
        }
    }

    #[cfg(feature = "integration-test")]
    #[gpui::test]
    fn terminal_graphics_bypass_fonts_but_keep_decorations_and_skip_cells(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                |bounds, _, window, cx| {
                    let mut frame = FrameData {
                        width: 5,
                        height: 1,
                        cells: vec![
                            CellData {
                                modifier: UNDERLINE | STRIKETHROUGH,
                                ..cell("▏")
                            },
                            CellData {
                                modifier: REVERSED,
                                ..cell("█")
                            },
                            CellData {
                                modifier: DIM,
                                ..cell("▀")
                            },
                            CellData {
                                modifier: HIDDEN,
                                ..cell("┼")
                            },
                            CellData {
                                skip: true,
                                ..cell("█")
                            },
                        ],
                        cursor: None,
                        hyperlinks: vec![],
                        graphics: vec![],
                    };
                    let mut painter = TerminalPainter::default();
                    for family in ["Menlo", "Courier"] {
                        painter.set_appearance(21.35, 30.5, Theme::default());
                        let before = *cx.default_global::<crate::performance::Counts>();
                        painter.paint_frame(
                            &frame,
                            bounds.origin,
                            None,
                            12.81,
                            &font(family),
                            &[],
                            &[],
                            None,
                            None,
                            window,
                            cx,
                        );
                        let after = cx.default_global::<crate::performance::Counts>();
                        assert_eq!(painter.glyphs.len(), 0);
                        assert_eq!(after.shapes, before.shapes);
                        assert_eq!(after.glyphs, before.glyphs);
                        assert_eq!(after.decorations - before.decorations, 2);
                        let backgrounds =
                            background_spans(&frame.cells, 0..frame.cells.len(), &painter.theme)
                                .count();
                        assert_eq!(after.quads - before.quads, backgrounds + 7);
                    }
                    frame.cells[0] = cell("a");
                    painter.paint_frame(
                        &frame,
                        bounds.origin,
                        None,
                        12.81,
                        &font("Menlo"),
                        &[],
                        &[],
                        None,
                        None,
                        window,
                        cx,
                    );
                    assert_eq!(
                        painter.glyphs.len(),
                        1,
                        "ordinary text still uses the glyph cache"
                    );
                },
            )
            .size_full()
        });
    }

    #[gpui::test]
    fn placed_images_decode_off_thread_then_paint_in_z_order(cx: &mut TestAppContext) {
        use herdr_client::{
            SurfaceImages,
            protocol::{
                SurfaceGraphicsAsset, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat,
                SurfaceGraphicsPlacement, SurfaceGraphicsSource, SurfaceGraphicsTarget,
            },
        };
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        let painter = std::rc::Rc::new(std::cell::RefCell::new(TerminalPainter::default()));
        painter
            .borrow_mut()
            .set_appearance(14., 20., Theme::default());
        let key = |image_id, target| SurfaceGraphicsAssetKey {
            source: SurfaceGraphicsSource::Terminal { target, image_id },
            image_width: 1,
            image_height: 1,
            format: SurfaceGraphicsFormat::Rgba,
            data_len: 4,
            data_fingerprint: u64::from(image_id),
        };
        let pane = || SurfaceGraphicsTarget::Pane {
            pane_id: "p1".into(),
        };
        let (above, below) = (key(1, pane()), key(2, pane()));
        let popup = key(
            3,
            SurfaceGraphicsTarget::Popup {
                terminal_id: "t".into(),
            },
        );
        let images: Arc<SurfaceImages> = Arc::new(
            [&above, &below, &popup]
                .into_iter()
                .map(|key| SurfaceGraphicsAsset {
                    key: key.clone(),
                    data: vec![1, 2, 3, 255],
                })
                .collect(),
        );
        let place = |key: &SurfaceGraphicsAssetKey, x, z| SurfaceGraphicsPlacement {
            asset: key.clone(),
            logical_placement_id: 1,
            x,
            y: 0,
            cols: 1,
            rows: 1,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            x_offset: 0,
            y_offset: 0,
            z,
            scrollback_offset: 0,
        };
        // Scene order is not paint order: z decides.
        let placements: Arc<[SurfaceGraphicsPlacement]> = Arc::from([
            place(&above, 0, 5),
            place(&below, 1, -1),
            place(&popup, 2, 0),
        ]);
        let frame = FrameData {
            width: 3,
            height: 1,
            cells: vec![cell("x"); 3],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        let draw = |cx: &mut VisualTestContext| {
            let (painter, images, placements, frame) = (
                painter.clone(),
                images.clone(),
                placements.clone(),
                frame.clone(),
            );
            cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
                canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        painter.borrow_mut().paint_frame(
                            &frame,
                            point(px(10.), px(10.)),
                            None,
                            10.,
                            &font("Menlo"),
                            &[],
                            &[],
                            None,
                            Some(PlacedImages {
                                placements: &placements,
                                images: &images,
                                target: ImageTarget::Main,
                            }),
                            window,
                            cx,
                        );
                    },
                )
                .size_full()
            });
        };
        draw(cx);
        assert!(
            painter.borrow().painted_images.is_empty(),
            "nothing paints before its decode finishes"
        );
        cx.run_until_parked();
        draw(cx);
        let cell_at = |x: f32| Bounds::new(point(px(x), px(10.)), size(px(10.), px(20.)));
        // The popup's image belongs to the popup frame, not the main grid.
        assert_eq!(
            painter.borrow().painted_images,
            [(-1, cell_at(20.)), (5, cell_at(10.))]
        );
    }

    #[gpui::test]
    fn cache_reuses_cells_invalidates_fonts_and_bounds_storage(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        let painter = std::rc::Rc::new(std::cell::RefCell::new(TerminalPainter::default()));
        let frame = FrameData {
            width: 5,
            height: 1,
            cells: vec![
                cell("x"),
                cell("x"),
                cell("e\u{301}"),
                cell("\u{754c}"),
                CellData {
                    skip: true,
                    ..cell("")
                },
            ],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        let mut draw = |frame: FrameData, font: Font| {
            let painter = painter.clone();
            cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let cell_width = painter.borrow_mut().cell_width(&font, window, cx);
                        painter.borrow_mut().paint_frame(
                            &frame,
                            bounds.origin,
                            None,
                            cell_width,
                            &font,
                            &[],
                            &[],
                            None,
                            None,
                            window,
                            cx,
                        );
                    },
                )
                .size_full()
            });
        };
        draw(frame.clone(), font("Menlo"));
        assert_eq!(painter.borrow().glyphs.len(), 3);
        let original_width = painter.borrow().cell_width.unwrap_or_default();
        painter
            .borrow_mut()
            .set_appearance(FONT_SIZE, CELL_HEIGHT, Theme::default());
        assert_eq!(
            painter.borrow().glyphs.len(),
            3,
            "unchanged appearance retains glyphs"
        );
        assert_eq!(painter.borrow().cell_width, Some(original_width));
        let mut theme = Theme::default();
        for (font_size, cell_height) in [(28., CELL_HEIGHT), (28., 36.), (28., 36.)] {
            // The final iteration changes only the palette.
            if painter.borrow().cell_height == 36. {
                theme.palette[1] = 0x123456;
            }
            painter
                .borrow_mut()
                .set_appearance(font_size, cell_height, theme.clone());
            assert_eq!(painter.borrow().glyphs.len(), 0);
            assert!(painter.borrow().cell_width.is_none());
            draw(frame.clone(), font("Menlo"));
            assert_eq!(painter.borrow().glyphs.len(), 3);
            assert!(painter.borrow().cell_width.unwrap_or_default() > original_width * 1.5);
            let painter = painter.borrow();
            for (_, _, line) in painter.glyphs.iter() {
                assert_eq!(line.font_size, px(font_size));
            }
        }
        painter
            .borrow_mut()
            .set_appearance(FONT_SIZE, CELL_HEIGHT, Theme::default());
        draw(frame.clone(), font("Menlo"));
        assert_eq!(painter.borrow().glyphs.len(), 3);
        let mut changed = frame.clone();
        changed.cells[0].fg = 0x02123456;
        changed.cells[1].modifier = 1 | 4;
        draw(changed, font("Menlo"));
        assert_eq!(
            painter.borrow().glyphs.len(),
            4,
            "a new color reuses the glyph; only bold italic shapes again"
        );
        draw(frame.clone(), font("Courier"));
        assert_eq!(
            painter.borrow().glyphs.len(),
            3,
            "new font discards old glyphs"
        );
        // A changed icon cascade reshapes every cell: the same family can now
        // resolve Private Use Area glyphs a text face does not carry.
        let with_fallbacks = |families: &[&str]| {
            crate::config::FontConfig {
                family: "Courier".into(),
                size: FONT_SIZE,
                fallbacks: Some(families.iter().map(|family| (*family).to_owned()).collect()),
            }
            .font()
        };
        let cascaded = with_fallbacks(&["Symbols Nerd Font Mono"]);
        assert_ne!(cascaded, font("Courier"));
        draw(frame.clone(), cascaded.clone());
        assert_eq!(
            painter.borrow().glyphs.len(),
            3,
            "an added cascade discards old glyphs"
        );
        assert_eq!(painter.borrow().config.as_ref(), Some(&cascaded));
        draw(frame.clone(), with_fallbacks(&["Hack Nerd Font Mono"]));
        assert_eq!(
            painter.borrow().glyphs.len(),
            3,
            "a reordered cascade discards old glyphs"
        );
        draw(frame, font("Courier"));
        let colors = FrameData {
            width: 100,
            height: 50,
            cells: (0..5000)
                .map(|i| CellData {
                    fg: 0x02000000 | i,
                    ..cell("x")
                })
                .collect(),
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        draw(colors, font("Menlo"));
        assert_eq!(
            painter.borrow().glyphs.len(),
            1,
            "truecolor output shares one glyph"
        );
        let symbols = FrameData {
            width: 100,
            height: 50,
            cells: (0..5000)
                .filter_map(|i| char::from_u32(0x4e00 + i))
                .map(|symbol| cell(&symbol.to_string()))
                .collect(),
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        draw(symbols, font("Menlo"));
        assert_eq!(painter.borrow().glyphs.len(), glyphs::CACHE_LIMIT);
        assert_eq!(painter.borrow().glyphs.iter().count(), glyphs::CACHE_LIMIT);
    }

    #[test]
    fn composition_stays_at_the_cursor_inside_the_grid() {
        let grid = Bounds::new(point(px(10.), px(20.)), size(px(100.), px(40.)));
        let cursor = point(px(30.), px(40.));
        assert_eq!(composition_origin(cursor, px(50.), grid), cursor);
        assert_eq!(
            composition_origin(point(px(90.), px(40.)), px(50.), grid),
            point(px(60.), px(40.)),
            "shifted left just enough to end at the grid's edge"
        );
        assert_eq!(
            composition_origin(cursor, px(200.), grid),
            point(px(-90.), px(40.)),
            "wider than the grid, it keeps its end, where the IME edits"
        );
    }

    #[test]
    fn composition_ranges_count_utf16_units() {
        let text = "a\u{304b}\u{1f600}b";
        assert_eq!(
            [0, 1, 2, 3, 4, 5, 6].map(|utf16| byte_index(text, utf16)),
            [0, 1, 4, 8, 8, 9, 9]
        );
        assert_eq!(byte_index("", 1), 0);
    }

    #[gpui::test]
    fn composition_bounds_follow_the_converted_clause(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                |bounds, _, window, cx| {
                    let mut painter = TerminalPainter::default();
                    let font = font("Menlo");
                    let cell_width = painter.cell_width(&font, window, cx);
                    let cursor = Bounds::new(
                        bounds.origin + point(px(cell_width * 4.), px(CELL_HEIGHT)),
                        size(px(cell_width), px(CELL_HEIGHT)),
                    );
                    // Romaji mid-composition: the headless text system gives
                    // CJK glyphs next to no advance, which would hide the
                    // right-edge geometry. UTF-16 mapping is tested separately.
                    let text = "kan";
                    let bounds_of = |range, cursor| {
                        painter.composition_bounds(text, range, cursor, bounds, &font, window)
                    };
                    assert_eq!(
                        painter.composition_bounds("", 0..0, cursor, bounds, &font, window),
                        cursor,
                        "without a composition the IME anchors at the cursor"
                    );
                    let first = bounds_of(0..1, cursor);
                    let second = bounds_of(1..2, cursor);
                    let caret = bounds_of(3..3, cursor);
                    assert_eq!(first.origin, cursor.origin);
                    assert_eq!(second.origin.y, cursor.origin.y);
                    assert!(second.origin.x > first.origin.x);
                    assert!(caret.origin.x > second.origin.x);
                    assert_eq!(caret.size, cursor.size);
                    // At the right edge the text, and so every clause, shifts left.
                    let edge = Bounds::new(
                        point(bounds.right() - px(cell_width), cursor.origin.y),
                        cursor.size,
                    );
                    let end = bounds_of(3..3, edge);
                    assert_eq!(end.right(), bounds.right(), "the caret stays inside");
                    assert_eq!(end.size, cursor.size);
                    assert!(bounds_of(0..1, edge).origin.x < edge.origin.x);
                    // Wider than the grid, the start scrolls off the left while
                    // every clause still anchors inside the grid.
                    let long = "k".repeat(200);
                    let long_bounds = |range| {
                        painter.composition_bounds(&long, range, cursor, bounds, &font, window)
                    };
                    assert_eq!(long_bounds(0..1).origin.x, bounds.left());
                    assert_eq!(long_bounds(200..200).right(), bounds.right());
                },
            )
            .size_full()
        });
    }

    #[test]
    fn spans_and_styles_use_custom_theme() {
        let mut theme = Theme {
            background: 0x123456,
            foreground: 0xabcdef,
            ..Theme::default()
        };
        theme.palette[200] = theme.background;
        theme.palette[1] = 0x654321;
        let row = [
            cell("x"),
            CellData {
                bg: 0x010000c8,
                fg: 2,
                ..cell("y")
            },
        ];
        assert_eq!(
            background_spans(&row, 0..row.len(), &theme).collect::<Vec<_>>(),
            vec![(0, 2, theme.background)]
        );
        assert_eq!(cell_colors(&row[0], &theme).0, theme.foreground);
        assert_eq!(cell_colors(&row[1], &theme).0, theme.palette[1]);
    }
}

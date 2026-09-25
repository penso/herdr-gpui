mod glyphs;
mod graphics;

use self::glyphs::GlyphCache;
use self::graphics::Graphic;
use crate::config::Theme;
use crate::terminal::*;
use gpui::*;
use herdr_client::protocol::{CellData, FrameData, PaneSurfacePane, SurfaceRect};
use std::time::{Duration, Instant};

/// The selection tints the cells it covers instead of replacing their colors:
/// a terminal's own background is meaningful, and the glyphs above it stay
/// readable on every theme.
const SELECTION_ALPHA: u32 = 0x59;
const REPORT_INTERVAL: Duration = Duration::from_secs(5);
const SLOW_PAINT: Duration = Duration::from_millis(16);
const SCROLLBAR_INSET: f32 = 1.;
const SCROLLBAR_ALPHA: u32 = 0xc0;

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
            #[cfg(feature = "integration-test")]
            uncached: false,
        }
    }
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
    theme: &'a Theme,
) -> impl Iterator<Item = (usize, usize, u32)> + 'a {
    let mut start = 0;
    std::iter::from_fn(move || {
        let color = cell_colors(row.get(start)?, theme).1;
        let mut end = start + 1;
        while end < row.len() && cell_colors(&row[end], theme).1 == color {
            end += 1;
        }
        let span = (start, end, color);
        start = end;
        Some(span)
    })
}

/// Where an IME composition sits: at the input cursor, shifted left only as
/// far as it takes to keep the text inside the grid.
fn composition_origin(cursor: Point<Pixels>, width: Pixels, grid: Bounds<Pixels>) -> Point<Pixels> {
    point(
        cursor.x.min(grid.right() - width).max(grid.left()),
        cursor.y,
    )
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

    /// Paints one frame, tinting the cells `selection` names in that frame's
    /// own grid. Rows outside the frame are ignored: the selection was made
    /// against the live surface, which a repaint may already have replaced.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_frame(
        &mut self,
        frame: &FrameData,
        origin: Point<Pixels>,
        cell_width: f32,
        font: &Font,
        selection: &[(u16, std::ops::Range<u16>)],
        panes: &[PaneSurfacePane],
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
        // The daemon's cell scrollbar is replaced by the pixel thumb painted below.
        let bars: Vec<SurfaceRect> = panes.iter().filter_map(|p| p.scrollbar_rect).collect();
        let in_bar = |index: usize| {
            let width = usize::from(frame.width);
            bars.iter()
                .any(|r| in_rect(*r, (index % width) as u16, (index / width) as u16))
        };
        // A layer gives all its primitives one draw order, skipping GPUI's
        // per-primitive BoundsTree insert that dominates large grids. Within a
        // layer quads draw before glyphs, so decorations and the cursor take a
        // second layer above the text.
        window.paint_layer(grid, |window| {
            // Backgrounds precede all glyphs, including wide graphemes' skip cells.
            for (y, row) in frame.cells.chunks(usize::from(frame.width)).enumerate() {
                let mut paint = |start: usize, end: usize, color| {
                    window.paint_quad(fill(
                        Bounds::new(
                            origin
                                + point(
                                    px(start as f32 * cell_width),
                                    px(y as f32 * self.cell_height),
                                ),
                            size(px((end - start) as f32 * cell_width), px(self.cell_height)),
                        ),
                        rgb(color),
                    ));
                    #[cfg(feature = "integration-test")]
                    {
                        counts.quads += 1;
                    }
                };
                if cached {
                    for (start, end, color) in background_spans(row, &self.theme) {
                        paint(start, end, color);
                    }
                } else {
                    for (x, cell) in row.iter().enumerate() {
                        paint(x, x + 1, cell_colors(cell, &self.theme).1);
                    }
                }
            }
            // Between the backgrounds and the glyphs, so the tint reads as chosen
            // without hiding either.
            for (row, columns) in selection {
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
                    rgba((self.theme.primary() << 8) | SELECTION_ALPHA),
                ));
                #[cfg(feature = "integration-test")]
                {
                    counts.quads += 1;
                }
            }
            for (index, cell) in frame.cells.iter().enumerate() {
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
        });
        window.paint_layer(grid, |window| {
            // Box and block graphics are quads, so they share this layer to stay
            // above the backgrounds. Decorations cover the grid, including spaces
            // and wide-glyph continuation cells.
            for (index, cell) in frame.cells.iter().enumerate() {
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
            if let Some(cursor) = frame
                .cursor
                .as_ref()
                .filter(|c| c.visible && c.x < frame.width && c.y < frame.height)
            {
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
            for bar in panes
                .iter()
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
        // cells and the cursor it covers.
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
        Bounds::new(
            origin + point(start, px(0.)),
            size((end - start).max(cursor.size.width), cursor.size.height),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

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
                            8.5,
                            &font("Menlo"),
                            &[],
                            &[],
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
            background_spans(&row, &theme).collect::<Vec<_>>(),
            vec![(0, 2, BACKGROUND), (2, 4, 0x123456)]
        );
        assert_eq!(background_spans(&[], &theme).count(), 0);
        for cells in row.chunks(2) {
            let expanded: Vec<_> = background_spans(cells, &theme)
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
                            12.81,
                            &font(family),
                            &[],
                            &[],
                            window,
                            cx,
                        );
                        let after = cx.default_global::<crate::performance::Counts>();
                        assert_eq!(painter.glyphs.len(), 0);
                        assert_eq!(after.shapes, before.shapes);
                        assert_eq!(after.glyphs, before.glyphs);
                        assert_eq!(after.decorations - before.decorations, 2);
                        let backgrounds = background_spans(&frame.cells, &painter.theme).count();
                        assert_eq!(after.quads - before.quads, backgrounds + 7);
                    }
                    frame.cells[0] = cell("a");
                    painter.paint_frame(
                        &frame,
                        bounds.origin,
                        12.81,
                        &font("Menlo"),
                        &[],
                        &[],
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
                            cell_width,
                            &font,
                            &[],
                            &[],
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
            point(px(10.), px(40.)),
            "wider than the grid, it starts at the grid's left edge"
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
                    let text = "\u{304b}\u{3093}\u{3058}";
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
                    assert!(end.origin.x <= bounds.right());
                    assert!(bounds_of(0..1, edge).origin.x < edge.origin.x);
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
            background_spans(&row, &theme).collect::<Vec<_>>(),
            vec![(0, 2, theme.background)]
        );
        assert_eq!(cell_colors(&row[0], &theme).0, theme.foreground);
        assert_eq!(cell_colors(&row[1], &theme).0, theme.palette[1]);
    }
}

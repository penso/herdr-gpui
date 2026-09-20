use crate::config::{Config, Theme};
use crate::terminal::*;
use gpui::*;
use herdr_client::protocol::{CellData, FrameData};
use std::collections::HashMap;

const CACHE_LIMIT: usize = 4096;
const RUN_CACHE_LIMIT: usize = 512;
const RUN_LENGTH_LIMIT: usize = 256;

type CellStyle = (u32, u16);
type RunKey = (String, CellStyle, u32);

pub(crate) struct TerminalPainter {
    font_size: f32,
    cell_height: f32,
    theme: Theme,
    config: Option<Font>,
    // Resolved foreground includes reverse, dim and hidden; only bold/italic
    // affect shaping. Decorations remain at exact cell-grid coordinates.
    lines: HashMap<CellStyle, HashMap<String, ShapedLine>>,
    entries: usize,
    cell_width: Option<f32>,
    runs: HashMap<RunKey, Option<ShapedLine>>,
    #[cfg(feature = "integration-test")]
    no_batch: Option<bool>,
    #[cfg(feature = "integration-test")]
    pub uncached: bool,
}

impl Default for TerminalPainter {
    fn default() -> Self {
        let font = Config::default().terminal;
        Self {
            font_size: font.size,
            cell_height: font.line_height(),
            theme: Theme::default(),
            config: None,
            lines: HashMap::new(),
            entries: 0,
            cell_width: None,
            runs: HashMap::new(),
            #[cfg(feature = "integration-test")]
            no_batch: None,
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

fn batchable(cell: &CellData) -> bool {
    !cell.skip
        && cell.modifier & (UNDERLINE | STRIKETHROUGH) == 0
        && cell.symbol.len() == 1
        && cell.symbol.as_bytes()[0].is_ascii_graphic()
}

fn row_has_batch_pair(row: &[CellData], theme: &Theme) -> bool {
    let mut previous = None;
    for cell in row {
        let current = batchable(cell).then(|| style(cell, theme));
        if current.is_some() && current == previous {
            return true;
        }
        previous = current;
    }
    false
}

fn paint_glyphs(
    shaped: &ShapedLine,
    position: Point<Pixels>,
    cell_height: f32,
    window: &mut Window,
    cx: &mut App,
    #[cfg(feature = "integration-test")] counts: &mut crate::performance::Counts,
) {
    let result = shaped.paint(position, px(cell_height), window, cx);
    #[cfg(not(feature = "integration-test"))]
    let _ = result;
    #[cfg(feature = "integration-test")]
    {
        counts.glyphs += shaped.runs.iter().map(|r| r.glyphs.len()).sum::<usize>();
        counts.paint_errors += usize::from(result.is_err());
    }
}

// Compare native layouts, not advances inferred from text. In particular, a
// fallback font, ligature or changed baseline must retain the per-cell path.
fn equivalent_run<'a>(
    run: &LineLayout,
    cells: impl IntoIterator<Item = &'a LineLayout>,
    width: f32,
    column: usize,
    origin_x: Pixels,
) -> bool {
    if run.runs.len() != 1 {
        return false;
    }
    let native = &run.runs[0];
    let mut count = 0;
    let mut painted_x = origin_x + px(column as f32 * width);
    let mut previous_x = px(0.);
    for (i, cell) in cells.into_iter().enumerate() {
        if cell.font_size != run.font_size
            || cell.width != px(width)
            || cell.ascent != run.ascent
            || cell.descent != run.descent
            || cell.runs.len() != 1
        {
            return false;
        }
        let single = &cell.runs[0];
        let Some(glyph) = native.glyphs.get(i) else {
            return false;
        };
        if single.font_id != native.font_id || single.glyphs.len() != 1 {
            return false;
        }
        let original = &single.glyphs[0];
        painted_x += glyph.position.x - previous_x;
        previous_x = glyph.position.x;
        if original.is_emoji
            || glyph.is_emoji
            || original.id != glyph.id
            || original.index != 0
            || glyph.index != i
            || glyph.position.y != original.position.y
            || glyph.position.x != px(i as f32 * width) + original.position.x
            || painted_x != (origin_x + px((column + i) as f32 * width)) + original.position.x
        {
            return false;
        }
        count += 1;
    }
    // GPUI uses layout width for paint_layer bounds, not just glyph placement.
    count >= 2 && count == native.glyphs.len() && run.width == px(count as f32 * width)
}

fn style(cell: &CellData, theme: &Theme) -> CellStyle {
    (cell_colors(cell, theme).0, cell.modifier & (BOLD | ITALIC))
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

impl TerminalPainter {
    pub fn set_appearance(&mut self, font_size: f32, cell_height: f32, theme: Theme) {
        if self.font_size != font_size || self.cell_height != cell_height || self.theme != theme {
            self.font_size = font_size;
            self.cell_height = cell_height;
            self.theme = theme;
            self.lines.clear();
            self.runs.clear();
            self.entries = 0;
            self.cell_width = None;
        }
    }

    #[cfg(feature = "integration-test")]
    pub fn reset_cache(&mut self) {
        self.config = None;
        self.lines.clear();
        self.runs.clear();
        self.entries = 0;
        self.cell_width = None;
    }

    #[cfg(feature = "integration-test")]
    pub fn verify_native_cache(&self, window: &Window) -> Result<usize, String> {
        let Some(base) = &self.config else {
            return Err("missing font config".into());
        };
        for ((color, flags), lines) in &self.lines {
            let mut font = base.clone();
            if flags & BOLD != 0 {
                font.weight = FontWeight::BOLD;
            }
            if flags & ITALIC != 0 {
                font.style = FontStyle::Italic;
            }
            for (symbol, cached) in lines {
                let fresh = window.text_system().shape_line(
                    symbol.clone().into(),
                    px(self.font_size),
                    &[TextRun {
                        len: symbol.len(),
                        font: font.clone(),
                        color: rgb(*color).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                // Includes native glyph IDs/positions, font IDs, metrics and colors.
                if format!("{fresh:?}") != format!("{cached:?}") {
                    return Err(format!("cached glyph/style mismatch: {symbol:?}"));
                }
            }
        }
        for ((text, key, width), cached) in &self.runs {
            let Some(run) = cached else { continue };
            let cells = text
                .bytes()
                .map(|byte| {
                    self.lines
                        .get(key)
                        .and_then(|lines| lines.get(&(byte as char).to_string()))
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| format!("missing batch cells: {text:?}"))?;
            if !equivalent_run(
                run,
                cells.iter().map(|line| -> &LineLayout { line }),
                f32::from_bits(*width),
                0,
                px(0.),
            ) {
                return Err(format!("cached batch/native cell mismatch: {text:?}"));
            }
        }
        Ok(self.entries)
    }
    fn configure(&mut self, font: &Font) {
        if self.config.as_ref() != Some(font) {
            self.lines.clear();
            self.runs.clear();
            self.entries = 0;
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

    #[allow(clippy::too_many_arguments)]
    pub fn paint_frame(
        &mut self,
        frame: &FrameData,
        origin: Point<Pixels>,
        cell_width: f32,
        font: &Font,
        window: &mut Window,
        cx: &mut App,
    ) {
        if frame.width == 0 {
            return;
        }
        self.configure(font);
        let cached = true;
        #[cfg(feature = "integration-test")]
        let cached = cached && !self.uncached;
        let batching = cached;
        #[cfg(feature = "integration-test")]
        let batching = batching
            && !*self
                .no_batch
                .get_or_insert_with(|| std::env::var_os("HERDR_PERF_NO_BATCH").is_some());
        #[cfg(feature = "integration-test")]
        let mut counts = crate::performance::Counts::default();
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
        for (y, row) in frame.cells.chunks(usize::from(frame.width)).enumerate() {
            let batching = batching && row_has_batch_pair(row, &self.theme);
            // Stage only eligible rows; other rows retain the borrowed-glyph path.
            let mut shaped_cells = if batching {
                vec![None; row.len()]
            } else {
                Vec::new()
            };
            for (index, cell) in row.iter().enumerate() {
                if cell.skip || cell.symbol.is_empty() || cell.symbol == " " {
                    continue;
                }
                let key = style(cell, &self.theme);
                let mut overflow = HashMap::new();
                let lines = if self.entries < CACHE_LIMIT || self.lines.contains_key(&key) {
                    self.lines.entry(key).or_default()
                } else {
                    &mut overflow
                };
                let existing = cached.then(|| lines.get(cell.symbol.as_str())).flatten();
                let newly_shaped;
                let shaped = if let Some(line) = existing {
                    line
                } else {
                    let mut font = font.clone();
                    if key.1 & BOLD != 0 {
                        font.weight = FontWeight::BOLD;
                    }
                    if key.1 & ITALIC != 0 {
                        font.style = FontStyle::Italic;
                    }
                    #[cfg(feature = "integration-test")]
                    {
                        counts.shapes += 1;
                    }
                    newly_shaped = window.text_system().shape_line(
                        cell.symbol.clone().into(),
                        px(self.font_size),
                        &[TextRun {
                            len: cell.symbol.len(),
                            font,
                            color: rgb(key.0).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    if cached && self.entries < CACHE_LIMIT {
                        self.entries += 1;
                        lines.entry(cell.symbol.clone()).or_insert(newly_shaped)
                    } else {
                        &newly_shaped
                    }
                };
                if batching {
                    shaped_cells[index] = Some(shaped.clone());
                } else {
                    let position = origin
                        + point(
                            px(index as f32 * cell_width),
                            px(y as f32 * self.cell_height),
                        );
                    paint_glyphs(
                        shaped,
                        position,
                        self.cell_height,
                        window,
                        cx,
                        #[cfg(feature = "integration-test")]
                        &mut counts,
                    );
                }
            }
            let mut index = 0;
            let mut next_candidate = 0;
            while index < shaped_cells.len() {
                let cell = &row[index];
                let Some(shaped) = shaped_cells[index].as_ref() else {
                    index += 1;
                    continue;
                };
                let key = style(cell, &self.theme);
                let position = origin
                    + point(
                        px(index as f32 * cell_width),
                        px(y as f32 * self.cell_height),
                    );
                let mut end = index + 1;
                if batching && index >= next_candidate && batchable(cell) {
                    while end < row.len()
                        && end - index < RUN_LENGTH_LIMIT
                        && batchable(&row[end])
                        && style(&row[end], &self.theme) == key
                    {
                        end += 1;
                    }
                }
                let mut batch = None;
                if end - index >= 2 && shaped_cells[index..end].iter().all(Option::is_some) {
                    next_candidate = end;
                    let text: String = row[index..end].iter().map(|c| c.symbol.as_str()).collect();
                    let run_key = (text, key, cell_width.to_bits());
                    if !self.runs.contains_key(&run_key) && self.runs.len() < RUN_CACHE_LIMIT {
                        // Only retain candidates whose original cells remain available
                        // for native verification (the cell cache may be full).
                        let retained = self.lines.get(&key).is_some_and(|lines| {
                            row[index..end]
                                .iter()
                                .all(|c| lines.contains_key(&c.symbol))
                        });
                        if retained {
                            let mut run_font = font.clone();
                            if key.1 & BOLD != 0 {
                                run_font.weight = FontWeight::BOLD;
                            }
                            if key.1 & ITALIC != 0 {
                                run_font.style = FontStyle::Italic;
                            }
                            let line = window.text_system().shape_line(
                                run_key.0.clone().into(),
                                px(self.font_size),
                                &[TextRun {
                                    len: run_key.0.len(),
                                    font: run_font,
                                    color: rgb(key.0).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            );
                            #[cfg(feature = "integration-test")]
                            {
                                counts.run_shapes += 1;
                            }
                            let verified = equivalent_run(
                                &line,
                                shaped_cells[index..end]
                                    .iter()
                                    .filter_map(|c| c.as_ref().map(|line| -> &LineLayout { line })),
                                cell_width,
                                0,
                                px(0.),
                            );
                            self.runs.insert(run_key.clone(), verified.then_some(line));
                        }
                    }
                    batch = self
                        .runs
                        .get(&run_key)
                        .and_then(|line| line.as_ref())
                        .filter(|line| {
                            equivalent_run(
                                line,
                                shaped_cells[index..end]
                                    .iter()
                                    .filter_map(|c| c.as_ref().map(|line| -> &LineLayout { line })),
                                cell_width,
                                index,
                                origin.x,
                            )
                        });
                }
                let painted = batch.unwrap_or(shaped);
                paint_glyphs(
                    painted,
                    position,
                    self.cell_height,
                    window,
                    cx,
                    #[cfg(feature = "integration-test")]
                    &mut counts,
                );
                #[cfg(feature = "integration-test")]
                {
                    counts.runs += usize::from(batch.is_some());
                }
                index = if batch.is_some() { end } else { index + 1 };
            }
        }
        // Decorations cover the grid, including spaces and wide-glyph continuation cells.
        for (index, cell) in frame.cells.iter().enumerate() {
            let position = origin
                + point(
                    px((index % usize::from(frame.width)) as f32 * cell_width),
                    px((index / usize::from(frame.width)) as f32 * self.cell_height),
                );
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
        #[cfg(feature = "integration-test")]
        {
            let total = cx.default_global::<crate::performance::Counts>();
            total.shapes += counts.shapes;
            total.run_shapes += counts.run_shapes;
            total.runs += counts.runs;
            total.quads += counts.quads;
            total.glyphs += counts.glyphs;
            total.decorations += counts.decorations;
            total.paint_errors += counts.paint_errors;
            total.paints += 1;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn has_batch_pair(frame: &FrameData) -> bool {
        frame.width != 0
            && frame
                .cells
                .chunks(usize::from(frame.width))
                .any(|row| row_has_batch_pair(row, &Theme::default()))
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
    fn batches_only_undecorated_printable_ascii() {
        assert!(batchable(&cell("x")));
        for symbol in ["", " ", "\t", "\n", "\u{7f}", "\u{754c}", "ab", "e\u{301}"] {
            assert!(!batchable(&cell(symbol)));
        }
        for modifier in [8, 256, 8 | 256] {
            assert!(!batchable(&CellData {
                modifier,
                ..cell("x")
            }));
        }
        assert!(!batchable(&CellData {
            skip: true,
            ..cell("x")
        }));
    }

    #[test]
    fn sparse_ascii_pair_only_stages_its_own_row() {
        let mut cells = vec![cell("\u{754c}"); 12];
        cells[5] = cell("x");
        cells[6] = cell("y");
        cells[8..].fill(CellData {
            modifier: 8,
            ..cell("x")
        });
        assert_eq!(
            cells
                .chunks(4)
                .map(|row| row_has_batch_pair(row, &Theme::default()))
                .collect::<Vec<_>>(),
            vec![false, true, false]
        );
        assert!(!row_has_batch_pair(&[], &Theme::default()));
        assert!(!row_has_batch_pair(&[cell("x")], &Theme::default()));
    }

    #[test]
    fn batch_pair_gate_respects_rows_eligibility_and_resolved_style() {
        let mut frame = FrameData {
            width: 2,
            height: 2,
            cells: vec![cell("x"); 4],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        assert!(has_batch_pair(&frame));
        frame.cells[0].fg = 0x02123456;
        frame.cells[3].fg = 0x02123456;
        assert!(!has_batch_pair(&frame), "matching cells straddle rows");
        frame.width = 4;
        assert!(has_batch_pair(&frame));
        for blocked in [
            cell(" "),
            cell("\u{754c}"),
            CellData {
                skip: true,
                ..cell("x")
            },
            CellData {
                modifier: 8,
                ..cell("x")
            },
            CellData {
                modifier: 256,
                ..cell("x")
            },
        ] {
            frame.cells[1] = blocked;
            assert!(!has_batch_pair(&frame));
        }
        frame.cells = (0..100)
            .map(|i| CellData {
                fg: 0x02000000 | (i % 2),
                ..cell("x")
            })
            .collect();
        assert!(
            !has_batch_pair(&frame),
            "alternating foregrounds use single-pass painting"
        );
        frame.cells = vec![cell("x"); 2];
        frame.cells[1].bg = 0x02123456;
        assert!(
            has_batch_pair(&frame),
            "background does not change glyph style"
        );
        frame.width = 1;
        assert!(!has_batch_pair(&frame));
        frame.width = 0;
        assert!(!has_batch_pair(&frame));
    }

    #[gpui::test]
    fn native_run_equivalence_is_exact_and_rejects_layout_changes(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                |_, _, window, _| {
                    let single = window.text_system().shape_line(
                        "x".into(),
                        px(FONT_SIZE),
                        &[TextRun {
                            len: 1,
                            font: font("Menlo"),
                            color: rgb(FOREGROUND).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    // Use a native glyph but explicit nondegenerate metrics/positions:
                    // NoopTextSystem cannot exercise real kerning and fallback changes.
                    let original = LineLayout {
                        font_size: px(14.),
                        width: px(8.),
                        ascent: px(11.),
                        descent: px(3.),
                        runs: single.runs.clone(),
                        len: 1,
                    };
                    let make_run = || {
                        let mut runs = original.runs.clone();
                        let mut second = runs[0].glyphs[0].clone();
                        second.index = 1;
                        second.position.x += px(8.);
                        runs[0].glyphs.push(second);
                        LineLayout {
                            font_size: original.font_size,
                            width: px(16.),
                            ascent: original.ascent,
                            descent: original.descent,
                            runs,
                            len: 2,
                        }
                    };
                    let accepted = make_run();
                    assert!(equivalent_run(
                        &accepted,
                        [&original, &original],
                        8.,
                        7,
                        px(0.5)
                    ));
                    let other = window.text_system().shape_line(
                        "\u{1f600}".into(),
                        px(FONT_SIZE),
                        &[TextRun {
                            len: 4,
                            font: font("Menlo"),
                            color: rgb(FOREGROUND).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    let mut wrong_id = make_run();
                    wrong_id.runs[0].glyphs[1].id = other.runs[0].glyphs[0].id;
                    assert_ne!(wrong_id.runs[0].glyphs[1].id, original.runs[0].glyphs[0].id);
                    assert!(!equivalent_run(
                        &wrong_id,
                        [&original, &original],
                        8.,
                        0,
                        px(0.)
                    ));
                    let mut multiple = LineLayout {
                        width: original.width,
                        runs: original.runs.clone(),
                        ..LineLayout::default()
                    };
                    multiple.font_size = original.font_size;
                    multiple.ascent = original.ascent;
                    multiple.descent = original.descent;
                    multiple.runs[0]
                        .glyphs
                        .push(original.runs[0].glyphs[0].clone());
                    assert!(!equivalent_run(
                        &accepted,
                        [&original, &multiple],
                        8.,
                        0,
                        px(0.)
                    ));
                    let wrong_cell_width = LineLayout {
                        width: px(f32::from_bits(8f32.to_bits() + 1)),
                        font_size: original.font_size,
                        ascent: original.ascent,
                        descent: original.descent,
                        runs: original.runs.clone(),
                        len: original.len,
                    };
                    assert!(!equivalent_run(
                        &accepted,
                        [&original, &wrong_cell_width],
                        8.,
                        0,
                        px(0.)
                    ));
                    for change in 0..11 {
                        let mut changed = make_run();
                        match change {
                            0 => changed.ascent += px(1.),
                            1 => changed.descent += px(1.),
                            2 => changed.font_size += px(1.),
                            3 => changed.runs[0].font_id = FontId(usize::MAX),
                            4 => changed.runs[0].glyphs[1].is_emoji = true,
                            5 => changed.runs[0].glyphs[1].index = 0,
                            6 => changed.runs[0].glyphs[1].position.y += px(1.),
                            7 => {
                                changed.runs[0].glyphs[1].position.x =
                                    px(f32::from_bits(8f32.to_bits() + 1))
                            }
                            8 => {
                                changed.runs[0].glyphs.pop();
                            }
                            9 => {
                                changed.runs.push(changed.runs[0].clone());
                            }
                            _ => changed.width = px(f32::from_bits(16f32.to_bits() + 1)),
                        }
                        assert!(
                            !equivalent_run(&changed, [&original, &original], 8., 0, px(0.)),
                            "change {change}"
                        );
                    }
                },
            )
            .size_full()
        });
    }

    #[gpui::test]
    fn candidate_cache_retains_acceptance_and_rejection_and_is_bounded(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Empty);
        cx.draw(Point::default(), size(px(800.), px(600.)), |_, _| {
            canvas(
                |_, _, _| (),
                |bounds, _, window, cx| {
                    let mut painter = TerminalPainter::default();
                    #[cfg(feature = "integration-test")]
                    {
                        painter.no_batch = Some(false);
                    }
                    let base = font("Menlo");
                    let width = painter.cell_width(&base, window, cx);
                    let frame = FrameData {
                        width: 2,
                        height: 1,
                        cells: vec![cell("x"), cell("x")],
                        cursor: None,
                        hyperlinks: vec![],
                        graphics: vec![],
                    };
                    painter.paint_frame(&frame, bounds.origin, width, &base, window, cx);
                    let key = (
                        "xx".to_owned(),
                        style(&frame.cells[0], &Theme::default()),
                        width.to_bits(),
                    );
                    assert!(painter.runs[&key].is_some());
                    painter.paint_frame(&frame, bounds.origin, width, &base, window, cx);
                    assert_eq!(painter.runs.len(), 1);
                    assert_eq!(painter.entries, 1);
                    let wrong_width = width + 1.;
                    painter.paint_frame(&frame, bounds.origin, wrong_width, &base, window, cx);
                    let rejected = (
                        "xx".to_owned(),
                        style(&frame.cells[0], &Theme::default()),
                        wrong_width.to_bits(),
                    );
                    assert!(painter.runs[&rejected].is_none());
                    painter.paint_frame(&frame, bounds.origin, wrong_width, &base, window, cx);
                    assert_eq!(painter.runs.len(), 2);
                    for i in 0..RUN_CACHE_LIMIT {
                        painter.paint_frame(
                            &frame,
                            bounds.origin,
                            wrong_width + i as f32,
                            &base,
                            window,
                            cx,
                        );
                    }
                    assert_eq!(painter.runs.len(), RUN_CACHE_LIMIT);
                    assert!(painter.runs[&key].is_some());
                    painter.set_appearance(FONT_SIZE, CELL_HEIGHT, Theme::default());
                    assert_eq!(painter.runs.len(), RUN_CACHE_LIMIT);
                    let mut theme = Theme::default();
                    theme.cursor ^= 0xffffff;
                    painter.set_appearance(FONT_SIZE, CELL_HEIGHT, theme);
                    assert!(painter.runs.is_empty());
                    assert!(painter.lines.is_empty());
                    assert!(painter.cell_width.is_none());
                    painter.paint_frame(&frame, bounds.origin, width, &base, window, cx);
                    assert_eq!(painter.runs.len(), 1);
                    painter.configure(&font("Courier"));
                    assert!(painter.runs.is_empty());
                    #[cfg(feature = "integration-test")]
                    {
                        painter.paint_frame(&frame, bounds.origin, width, &base, window, cx);
                        painter.verify_native_cache(window).unwrap();
                        painter.reset_cache();
                        assert!(painter.runs.is_empty());
                    }
                },
            )
            .size_full()
        });
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
                        painter.paint_frame(&frame, bounds.origin, 8.5, &font("Menlo"), window, cx);
                        assert_eq!(painter.entries, 0);
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
    fn cache_style_includes_resolved_color_and_font_but_not_grid_decorations() {
        let theme = Theme::default();
        let base = cell("e\u{301}");
        let mut changed = base.clone();
        changed.modifier = 8 | 256;
        changed.hyperlink = Some(1);
        assert_eq!(style(&base, &theme), style(&changed, &theme));
        for modifier in [1, 4, 2, 64, 128] {
            changed.modifier = modifier;
            assert_ne!(style(&base, &theme), style(&changed, &theme));
        }
        changed.modifier = 0;
        changed.bg = 0x02abcdef;
        assert_eq!(style(&base, &theme), style(&changed, &theme));
        changed.fg = 0x02123456;
        assert_ne!(style(&base, &theme), style(&changed, &theme));
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
                            window,
                            cx,
                        );
                    },
                )
                .size_full()
            });
        };
        draw(frame.clone(), font("Menlo"));
        assert_eq!(painter.borrow().entries, 3);
        let original_width = painter.borrow().cell_width.unwrap_or_default();
        painter
            .borrow_mut()
            .set_appearance(FONT_SIZE, CELL_HEIGHT, Theme::default());
        assert_eq!(
            painter.borrow().entries,
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
            assert_eq!(painter.borrow().entries, 0);
            assert!(painter.borrow().lines.is_empty());
            assert!(painter.borrow().runs.is_empty());
            assert!(painter.borrow().cell_width.is_none());
            draw(frame.clone(), font("Menlo"));
            assert_eq!(painter.borrow().entries, 3);
            assert!(painter.borrow().cell_width.unwrap_or_default() > original_width * 1.5);
            let painter = painter.borrow();
            for lines in painter.lines.values() {
                for line in lines.values() {
                    assert_eq!(line.font_size, px(font_size));
                }
            }
        }
        painter
            .borrow_mut()
            .set_appearance(FONT_SIZE, CELL_HEIGHT, Theme::default());
        draw(frame.clone(), font("Menlo"));
        assert_eq!(painter.borrow().entries, 3);
        let mut changed = frame.clone();
        changed.cells[0].fg = 0x02123456;
        changed.cells[1].modifier = 1 | 4;
        draw(changed, font("Menlo"));
        assert_eq!(painter.borrow().entries, 5);
        draw(frame, font("Courier"));
        assert_eq!(painter.borrow().entries, 3, "new font discards old glyphs");
        let many = FrameData {
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
        draw(many, font("Menlo"));
        assert_eq!(painter.borrow().entries, CACHE_LIMIT);
        assert_eq!(painter.borrow().lines.len(), CACHE_LIMIT);
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
        assert_eq!(style(&row[0], &theme).0, theme.foreground);
        assert_eq!(style(&row[1], &theme).0, theme.palette[1]);
        assert!(!row_has_batch_pair(&row, &theme));
        theme.palette[1] = theme.foreground;
        assert!(row_has_batch_pair(&row, &theme));
    }
}

//! Native check of bidirectional text in the platform's own shaper
//! (docs/i18n.md, Phase 4). The headless test text system gives every glyph
//! the same advance in byte order, so only a real one can show whether
//! Fluent's isolation marks draw as nothing, a Hebrew run is drawn right to
//! left, and the field caret follows it.
use super::*;

/// What the platform did with each case, for the log.
#[derive(Debug)]
pub(super) struct Report {
    /// Hebrew drawn right to left, as CoreText and cosmic-text draw it.
    /// GPUI's DirectWrite path places glyphs in the order it is handed them
    /// and ignores their bidi level, so Windows reports `false` until GPUI
    /// reads it.
    pub(super) visual_order: bool,
    /// An `en-XB` message with two numbers, as the glyphs are drawn.
    pub(super) pseudo: String,
}

fn shape(text: &str, window: &mut Window) -> ShapedLine {
    let font = window.text_style().font();
    window.text_system().shape_line(
        text.to_owned().into(),
        px(14.),
        &[TextRun {
            len: text.len(),
            font,
            color: rgb(0xffffff).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    )
}

/// The characters of `line` from left to right as its glyphs are placed.
fn drawn(line: &ShapedLine) -> String {
    let mut glyphs: Vec<(f32, usize)> = line
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| (f32::from(glyph.position.x), glyph.index))
        .collect();
    glyphs.sort_by(|a, b| a.0.total_cmp(&b.0));
    glyphs
        .into_iter()
        .filter_map(|(_, index)| line.text.get(index..)?.chars().next())
        .filter(|ch| !matches!(ch, '\u{2066}'..='\u{2069}' | '\u{202a}'..='\u{202e}'))
        .collect()
}

pub(super) fn check(window: &mut Window) -> Result<Report> {
    let missing: Vec<GlyphId> = shape("\u{f0000}", window)
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
        .collect();
    // Fluent wraps every placeable in FSI and PDI: they must take no room
    // and never draw as the missing-glyph box.
    let plain = shape("ab", window);
    let isolated = shape("a\u{2068}b\u{2069}", window);
    if (plain.width - isolated.width).abs() > px(0.5) {
        bail!(
            "isolation marks take room: {:?} against {:?}",
            isolated.width,
            plain.width
        );
    }
    let boxes = isolated
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .any(|glyph| missing.contains(&glyph.id));
    if boxes {
        bail!("isolation marks draw as the missing-glyph box");
    }
    let hebrew = shape("\u{5d0}\u{5d1}\u{5d2}", window);
    let x = |line: &ShapedLine, index: usize| {
        line.runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .find(|glyph| glyph.index == index)
            .map(|glyph| glyph.position.x)
    };
    let visual_order = match (x(&hebrew, 0), x(&hebrew, 4)) {
        (Some(alef), Some(gimel)) => alef > gimel,
        _ => bail!("Hebrew shaped without its glyphs"),
    };
    if visual_order {
        // The caret before alef is its right edge, and moving on through the
        // word goes left, toward the Latin after it.
        let mixed = "ab \u{5d0}\u{5d1} cd";
        let line = shape(mixed, window);
        let carets = crate::caret::Carets::new(mixed, &line);
        if carets.x(3) <= carets.x(5) || carets.x(5) >= carets.x(7) {
            bail!(
                "caret does not follow the Hebrew run: {:?} {:?} {:?}",
                carets.x(3),
                carets.x(5),
                carets.x(7)
            );
        }
    }
    let pseudo = shape(
        "\u{202e}\u{2068}838\u{2069} of \u{2068}838\u{2069} themes\u{202c}",
        window,
    );
    Ok(Report {
        visual_order,
        pseudo: drawn(&pseudo),
    })
}

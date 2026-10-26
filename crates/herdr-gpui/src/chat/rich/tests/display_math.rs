use super::*;

#[test]
fn multiline_display_math_precedes_markdown_and_preserves_the_exact_tex_source() {
    let source = "\n\\hat x_{\\rm norm}\n=\n\\frac{c_{21}}{1.315}\n-\\tanh(p_{10,3}-0.2266).\n";
    for (open, close) in [("$$", "$$"), (r"\[", r"\]")] {
        let message = format!("Before\n\n{open}{source}{close}\n\nAfter");
        let blocks = parse(&message, &mut Visuals::default()).unwrap();
        assert!(
            matches!(&blocks[..], [Block::Paragraph { kind: Kind::Paragraph, .. }, Block::Math { source: actual, .. }, Block::Paragraph { kind: Kind::Paragraph, .. }] if actual.as_ref() == source)
        );
        assert!(draw_math(source, MathMode::Display, 16., 0xffffff).is_ok());
    }
}

#[test]
fn display_protection_keeps_comments_blank_lines_and_alignment_commands() {
    let source = "\nx = 1 % this comment ends here\n+ 2\n\n\\begin{aligned}a &= b \\\\\nc &= d\\end{aligned}\n";
    let blocks = parse(&format!("$${source}$$"), &mut Visuals::default()).unwrap();
    assert!(
        matches!(&blocks[..], [Block::Math { source: actual, .. }] if actual.as_ref() == source)
    );
}

#[test]
fn equation_zoom_keeps_scaling_above_the_previous_thirty_two_pixel_cap() {
    let normal = draw_math("x^2", MathMode::Display, 32., 0xffffff).unwrap();
    let large = draw_math("x^2", MathMode::Display, 50., 0xffffff).unwrap();
    assert!(large.width > normal.width * 1.4);
    assert!(large.height > normal.height * 1.4);
}

#[test]
fn math_lowercase_ink_has_the_optical_height_of_body_text() {
    for size in [20., 40.] {
        for mode in [MathMode::Inline, MathMode::Display] {
            let drawn = draw_math("x", mode, size, 0xffffff).unwrap();
            let pixels = drawn.texture.as_bytes(0).unwrap();
            let row_bytes = (drawn.width * 2.) as usize * 4;
            let ink_height = pixels
                .chunks_exact(row_bytes)
                .filter(|row| row.chunks_exact(4).any(|pixel| pixel[3] >= 64))
                .count() as f32
                / 2.;
            // Typical sans prose has a lowercase height near .54em. Measure
            // the raster's visible ink, not its transparent box or em size.
            assert!(
                (size * 0.50..=size * 0.60).contains(&ink_height),
                "lowercase ink {ink_height}px for {size}px body text"
            );
        }
    }
}

#[test]
fn formulas_do_not_reinterpret_fenced_inline_or_indented_code() {
    let source = "`$$x$$`\n\n```tex\n$$\nx\n=\ny\n$$\n```\n\n    $$x = y$$\n\n$$\nz\n=\n2\n$$";
    let blocks = parse(source, &mut Visuals::default()).unwrap();
    assert_eq!(
        blocks
            .iter()
            .filter(|block| matches!(block, Block::Math { .. }))
            .count(),
        1
    );
    let codes: Vec<_> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Code { source, .. } => Some(source.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(codes, ["$$\nx\n=\ny\n$$\n", "$$x = y$$\n"]);
}

#[test]
fn streaming_and_literal_markers_do_not_alias_complete_formulas() {
    let first = parse(
        "$$\nx\n=\ny\n$$\n\n$$HERDRDISPLAY$$\n\n$$\nz",
        &mut Visuals::default(),
    )
    .unwrap();
    let formulas: Vec<_> = first
        .iter()
        .filter_map(|block| match block {
            Block::Math { source, .. } => Some(source.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(formulas, ["\nx\n=\ny\n", "HERDRDISPLAY"]);
    let escaped = r"\$$ literal dollars and $$$ not a pair $$$";
    assert_eq!(
        math_input::protect_display(&parse::normalize_math(escaped)).markdown,
        escaped
    );
}

#[test]
fn an_unfinished_formula_cannot_borrow_its_closer_from_code_or_html() {
    for content in [
        "Unfinished \\[\n\n```tex\n\\]\n```",
        "Unfinished \\[\n\n<div>\\]</div>",
        "Unfinished \\[ `\\]`",
    ] {
        assert_eq!(parse::normalize_math(content).text, content);
    }
    let source = "\n\\begin{aligned}\n\n    a &= b \\\\\n    c &= d\n\\end{aligned}\n";
    let blocks = parse(&format!("\\[{source}\\]"), &mut Visuals::default()).unwrap();
    assert!(
        matches!(&blocks[..], [Block::Math { source: actual, .. }] if actual.as_ref() == source)
    );
}

#[test]
fn image_source_never_leaks_display_placeholders_or_normalized_delimiters() {
    for source in [
        "![formula $$\nx^2\n$$](local.png)",
        "![`code` \\[\nx^2\n\\]](local.png)",
    ] {
        let blocks = parse(source, &mut Visuals::default()).unwrap();
        let visible: String = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph { content, .. } => Some(
                    content
                        .iter()
                        .filter_map(|part| match part {
                            Inline::Text(text, _) => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect();
        assert_eq!(visible, source);
    }
}

#[test]
fn quoted_display_formulas_keep_math_comparisons_but_not_markdown_markers() {
    for prefix in ["> ", "> > "] {
        let text = format!("{prefix}$$\n{prefix}x > 1\n{prefix}=\n{prefix}y\n{prefix}$$");
        let blocks = parse(&text, &mut Visuals::default()).unwrap();
        assert!(
            matches!(&blocks[..], [Block::Math { source, .. }] if source.as_ref() == "\nx > 1\n=\ny\n")
        );
    }
}

#[test]
fn normalizing_math_keeps_code_and_html_where_a_fresh_parse_finds_them() {
    for text in [
        r"Area \( \pi r^2 \) and `\(code\)` then <b>\[x\]</b> \[ y \]",
        "Unfinished \\[\n\n```tex\n\\]\n```\n\\(a\\)",
        r"\(α\) ![\(img\)](x.png) \(β\)",
    ] {
        let normalized = parse::normalize_math(text);
        assert_eq!(
            normalized.protected,
            math_input::protected_ranges(&normalized.text),
            "{text}"
        );
    }
}

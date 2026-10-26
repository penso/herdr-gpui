#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::chat::visual::{Error, Visuals};

mod code;
mod display_math;
mod prose;

#[test]
fn inline_and_display_math_keep_their_place_and_identity_while_streaming() {
    let mut visuals = Visuals::default();
    let blocks = parse(
        "Before $x^2$ after.\n\n$$\\frac{a}{b}$$\n\nMore",
        &mut visuals,
    )
    .unwrap();
    let Block::Paragraph { content, .. } = &blocks[0] else {
        panic!("paragraph")
    };
    assert!(
        matches!(&content[..], [Inline::Text(a, _), Inline::Math { .. }, Inline::Text(b, _)] if a == "Before " && b == " after.")
    );
    assert!(matches!(&blocks[1], Block::Math { source, .. } if &**source == r"\frac{a}{b}"));
    let first = blocks[0].visuals().next().unwrap();
    let extended = parse("Before $x^2$ after. More streamed prose", &mut visuals).unwrap();
    assert_eq!(first, extended[0].visuals().next().unwrap());
    let unfinished = parse("Before $x^", &mut visuals).unwrap();
    assert!(
        matches!(&unfinished[0], Block::Paragraph { content, .. }
        if matches!(&content[..], [Inline::Text(text, _)] if text == "Before $x^")),
        "unfinished math stays literal"
    );
}

#[test]
fn alternate_delimiters_are_math_but_code_is_never_rewritten() {
    let mut visuals = Visuals::default();
    let blocks = parse(
        "Use \\(x_1\\) then \\[x^2\\]. `\\(literal\\)`\n\n```rust\nlet latex = \"$x$\";\n```",
        &mut visuals,
    )
    .unwrap();
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block, Block::Math { .. }))
    );
    let code = blocks
        .iter()
        .find_map(|block| match block {
            Block::Code { source, .. } => Some(source),
            _ => None,
        })
        .unwrap();
    assert_eq!(&**code, "let latex = \"$x$\";\n");
    assert!(blocks.iter().filter_map(|block| match block { Block::Paragraph { content, .. } => Some(content), _ => None })
        .flatten().any(|part| matches!(part, Inline::Text(text, style) if style.code && text == r"\(literal\)")));
}

#[test]
fn a_table_has_cells_alignment_and_inline_content() {
    let blocks = parse(
        "| Name | Value |\n| :--- | ---: |\n| **α** | $x^2$ |",
        &mut Visuals::default(),
    )
    .unwrap();
    let Block::Table { align, rows } = &blocks[0] else {
        panic!("table")
    };
    assert_eq!(align, &[Alignment::Left, Alignment::Right]);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.len() == 2));
    assert!(matches!(&rows[1][0][0], Inline::Text(text, style) if text == "α" && style.bold));
    assert!(matches!(&rows[1][1][0], Inline::Math { .. }));
}

#[test]
fn code_is_copied_verbatim_and_only_complete_fences_queue_grammars() {
    let mut visuals = Visuals::default();
    let open = parse("```rust\nfn main() {\n    let α = 2;", &mut visuals).unwrap();
    let Block::Code { id, source, .. } = &open[0] else {
        panic!("code")
    };
    assert_eq!(&**source, "fn main() {\n    let α = 2;");
    assert!(!visuals.wants(*id));
    let closed = parse("```rust\nfn main() {\n    let α = 2;\n}\n```", &mut visuals).unwrap();
    let Block::Code {
        id,
        source,
        language,
    } = &closed[0]
    else {
        panic!("code")
    };
    assert!(visuals.wants(*id));
    let spans = highlight(language, source);
    assert!(
        spans
            .iter()
            .flatten()
            .any(|span| span.token == crate::review::highlight::Token::Keyword)
    );
    for (line, spans) in source.lines().zip(spans) {
        assert!(
            spans
                .iter()
                .all(|span| line.is_char_boundary(span.start) && line.is_char_boundary(span.end))
        );
    }
}

#[test]
fn math_limits_and_errors_preserve_a_readable_source_path() {
    let nested_groups = format!("{}x{}", "{".repeat(65), "}".repeat(65));
    let nested_fractions = format!("{}x{}", r"\frac{1}{".repeat(65), "}".repeat(65));
    let nested_roots = format!("{}x", r"\sqrt".repeat(65));
    for source in [nested_groups, nested_fractions, nested_roots] {
        assert!(matches!(
            draw_math(&source, MathMode::Inline, 16., 0xffffff),
            Err(Error::MathLimit)
        ));
    }
    assert!(matches!(
        draw_math(
            &"x".repeat(MAX_MATH_BYTES + 1),
            MathMode::Inline,
            16.,
            0xffffff
        ),
        Err(Error::MathLimit)
    ));
    assert!(matches!(
        draw_math(r"\notARealCommand{", MathMode::Inline, 16., 0xffffff),
        Err(Error::MathParse(_))
    ));
    assert!(matches!(
        draw_math(
            r"\rule{100000em}{100000em}",
            MathMode::Inline,
            16.,
            0xffffff
        ),
        Err(Error::MathLimit) | Err(Error::Math(_))
    ));
    let inline = draw_math(r"\frac{a}{b}", MathMode::Inline, 16., 0xaabbcc).unwrap();
    let display = draw_math(r"\frac{a}{b}", MathMode::Display, 16., 0xaabbcc).unwrap();
    assert!(
        display.height > inline.height,
        "display fractions use display style"
    );
    assert!(
        inline
            .baseline
            .is_some_and(|baseline| baseline > 0. && baseline < inline.height)
    );
    let pixels = inline.texture.as_bytes(0).unwrap();
    assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
    assert!(
        pixels
            .chunks_exact(4)
            .any(|p| p[3] > 0 && p[..3] == [0xcc, 0xbb, 0xaa])
    );
}

#[test]
fn excessive_word_and_table_layouts_use_source_fallback() {
    assert!(
        parse(
            &format!("$x$ {}", "word ".repeat(4_097)),
            &mut Visuals::default()
        )
        .is_none()
    );
    let table = format!("| h |\n|---|\n{}", "| value |\n".repeat(129));
    assert!(parse(&table, &mut Visuals::default()).is_none());
}

#[test]
fn a_message_full_of_unmatched_math_openers_stays_literal_with_bounded_work() {
    let text = r"\(\[".repeat(65_536);
    let started = std::time::Instant::now();
    assert_eq!(parse::normalize_math(&text).text, text);
    // A generous regression budget, not a rendering-performance benchmark.
    // The former repeated suffix search scans tens of GiB for this 256 KiB input.
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let text = format!("{} `\\)` \\(α\\)", r"\[".repeat(32_768));
    let normalized = parse::normalize_math(&text).text;
    assert!(normalized.ends_with("`\\)` $α$"));
    assert_eq!(parse::normalize_math(r"\(x\\)y\)").text, r"$x\\)y$");
}

#[test]
fn long_prose_around_math_keeps_its_suffix() {
    let prose = format!("{}終わり", "α".repeat(17_000));
    let blocks = parse(&format!("{prose} $x$ suffix"), &mut Visuals::default()).unwrap();
    let Block::Paragraph { content, .. } = &blocks[0] else {
        panic!("paragraph")
    };
    assert!(matches!(&content[0], Inline::Text(text, _) if text == &format!("{prose} ")));
    assert!(matches!(content.last(), Some(Inline::Text(text, _)) if text == " suffix"));
}

#[test]
fn rich_prose_links_explicit_files_but_never_code_or_bare_file_urls() {
    let text = "See [docs](https://herdr.dev/a?b=1) or https://example.com/x). \
                [local](/tmp/file.rs:12) file:///tmp/file.rs \
                `https://in.code` and $x$.";
    let blocks = parse(text, &mut Visuals::default()).unwrap();
    let urls: Vec<_> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { content, .. } => Some(content),
            _ => None,
        })
        .flatten()
        .filter_map(|part| match part {
            Inline::Text(_, style) => style.link.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(
        urls,
        [
            "https://herdr.dev/a?b=1",
            "https://example.com/x",
            "/tmp/file.rs:12"
        ]
    );
    let plain = parse("See https://example.com/x", &mut Visuals::default()).unwrap();
    assert!(matches!(&plain[0], Block::Paragraph { content, .. }
        if content.iter().any(|part| matches!(part, Inline::Text(_, style)
            if style.link.as_deref() == Some("https://example.com/x")))));
}

#[test]
fn spaced_inline_delimiters_still_become_math() {
    assert_eq!(
        parse::normalize_math(r"Area \( \pi r^2 \).").text,
        r"Area $\pi r^2$."
    );
    assert_eq!(
        parse::normalize_math(r"Empty \(  \) stays").text,
        r"Empty \(  \) stays"
    );
    let blocks = parse(r"The area is \( \pi r^2 \).", &mut Visuals::default()).unwrap();
    let Block::Paragraph { content, .. } = &blocks[0] else {
        panic!("paragraph")
    };
    assert!(
        content
            .iter()
            .any(|part| matches!(part, Inline::Math { .. }))
    );
}

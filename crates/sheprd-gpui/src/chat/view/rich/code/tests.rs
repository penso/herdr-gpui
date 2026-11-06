use super::*;
use crate::{chat::rich::TAB_WIDTH, review::highlight::Token};

mod truncation;

#[core::prelude::v1::test]
fn changed_rows_and_syntax_tokens_keep_contrast_in_light_and_dark_themes() {
    for name in ["Default", "Catppuccin Latte"] {
        let Some(theme) = Theme::builtin(name) else {
            panic!("builtin theme")
        };
        for kind in [
            DiffKind::Added,
            DiffKind::Removed,
            DiffKind::Context,
            DiffKind::Meta,
        ] {
            let (background, ink) = diff_colors(&theme, Some(kind));
            assert!(
                crate::contrast::ratio(ink, background) >= 4.5,
                "{name}: {kind:?}"
            );
            if let Some(sign) = diff_sign(&theme, Some(kind), background) {
                assert!(crate::contrast::ratio(sign, background) >= 4.5);
                assert_ne!(background, theme.surface);
            } else {
                assert_eq!(background, theme.surface);
            }
            for token in [
                Token::Comment,
                Token::String,
                Token::Number,
                Token::Constant,
                Token::Keyword,
                Token::Type,
                Token::Function,
            ] {
                let ink = token.colour(&theme, Contrast::On(background));
                assert!(crate::contrast::ratio(ink, background) >= 4.5);
            }
        }
        assert_ne!(
            diff_colors(&theme, Some(DiffKind::Added)).0,
            diff_colors(&theme, Some(DiffKind::Removed)).0
        );
    }
}

#[core::prelude::v1::test]
fn tabs_keep_code_indentation_and_unicode_highlight_offsets() {
    let raw = "\tlet α = 2;\u{202e}";
    let keyword = Span {
        start: 1,
        end: 4,
        token: Token::Keyword,
    };
    let number = Span {
        start: 10,
        end: 11,
        token: Token::Number,
    };
    let line = code_line(raw, &[keyword, number]);
    assert_eq!(line.text, "    let α = 2;");
    assert_eq!(line.spans.len(), 2);
    assert_eq!(&line.text[line.spans[0].start..line.spans[0].end], "let");
    assert_eq!(&line.text[line.spans[1].start..line.spans[1].end], "2");
    assert!(!line.truncated);
    assert_eq!(code_line("界\tx", &[]).text, "界  x");
    assert_eq!(code_line("e\u{301}\tx", &[]).text, "e\u{301}   x");
}

#[core::prelude::v1::test]
fn clipped_code_keeps_prefix_highlights_and_reports_its_limit() {
    let raw = format!("\t{}tail", "α".repeat(MAX_CODE_LINE_CHARS - 1));
    let line = code_line(
        &raw,
        &[Span {
            start: 1,
            end: raw.len(),
            token: Token::String,
        }],
    );
    assert!(line.truncated);
    assert_eq!(
        line.text.chars().count(),
        MAX_CODE_LINE_CHARS + TAB_WIDTH - 1
    );
    assert!(line.text.starts_with("    α"));
    assert!(line.text.ends_with('α'));
    assert_eq!(
        line.spans,
        [Span {
            start: TAB_WIDTH,
            end: line.text.len(),
            token: Token::String
        }]
    );
    assert!(!code_line(&"x".repeat(MAX_CODE_LINE_CHARS), &[]).truncated);
    assert!(
        code_line(
            "α",
            &[Span {
                start: 1,
                end: 2,
                token: Token::String
            }]
        )
        .spans
        .is_empty()
    );
}

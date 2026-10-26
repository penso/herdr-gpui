//! Structured Markdown kept separately from layout. Parsing is bounded by
//! the message limit; math rasterization and grammar parsing use Visuals' workers.

mod code;
mod fence;
mod math;
mod math_input;
mod parse;
mod quote;
mod remote;

pub(super) use code::{CodeText, MAX_CODE_LINE_CHARS, MAX_CODE_LINES, TAB_WIDTH};
pub(super) use fence::Fence;
pub(super) use math::draw as draw_math;
pub(super) use parse::parse;
pub(super) use remote::remote_content;

use super::visual::{Source, VisualId, Visuals};
use pulldown_cmark::{Alignment, HeadingLevel};
use std::sync::Arc;

pub(super) const MAX_MATH_BYTES: usize = 4_096;
pub(super) const MAX_MATH: usize = 64;
pub(super) const MAX_CODE_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

/// How a formula is typeset: within a line, or set apart as its own display.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum MathMode {
    Inline,
    Display,
}

#[derive(Clone, Debug)]
pub(crate) enum Inline {
    Text(String, Style),
    Math { id: VisualId, source: Arc<str> },
    Break,
}

/// What a paragraph block is, which sets its typography and marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Paragraph,
    Heading(HeadingLevel),
    Bullet,
    /// An ordered list item, with the number it was written with.
    Numbered(u32),
    Quote,
    /// A thematic break, which has no content.
    Rule,
}

#[derive(Debug)]
pub(crate) enum Block {
    Paragraph {
        kind: Kind,
        /// Nested list depth; bounded in the view so deep input stays readable.
        indent: usize,
        content: Vec<Inline>,
    },
    Math {
        id: VisualId,
        source: Arc<str>,
    },
    Code {
        id: VisualId,
        language: String,
        source: Arc<str>,
    },
    Table {
        align: Vec<Alignment>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
}

impl Block {
    /// The visuals this block draws: its own code or display formula, or the
    /// inline formulas in its text.
    pub(super) fn visuals(&self) -> impl Iterator<Item = VisualId> + '_ {
        let (own, prose, rows): (_, &[Inline], &[Vec<Vec<Inline>>]) = match self {
            Self::Code { id, .. } | Self::Math { id, .. } => (Some(*id), &[], &[]),
            Self::Paragraph { content, .. } => (None, content, &[]),
            Self::Table { rows, .. } => (None, &[], rows),
        };
        let inline = prose.iter().chain(rows.iter().flatten().flatten());
        own.into_iter().chain(inline.filter_map(|part| match part {
            Inline::Math { id, .. } => Some(*id),
            _ => None,
        }))
    }
}

fn formula(
    source: &str,
    mode: MathMode,
    count: &mut usize,
    visuals: &mut Visuals,
) -> Option<(VisualId, Arc<str>)> {
    if source.len() > MAX_MATH_BYTES || source.trim().is_empty() || *count >= MAX_MATH {
        return None;
    }
    *count += 1;
    let source: Arc<str> = source.into();
    let id = VisualId::of(("math", mode, &source));
    visuals.want(
        id,
        Source::Math {
            source: source.clone(),
            mode,
        },
    );
    Some((id, source))
}

/// Reuse the review's syntax grammar cache and semantic token classes.
pub(super) fn highlight(language: &str, source: &str) -> Vec<Vec<crate::review::highlight::Span>> {
    let extension = match language {
        "rust" => "rs",
        "python" | "py" => "py",
        "javascript" | "js" | "jsx" => "js",
        "typescript" | "ts" | "tsx" => "ts",
        "bash" | "sh" | "shell" | "zsh" => "sh",
        "c++" | "cpp" => "cpp",
        "c#" | "csharp" => "cs",
        "ruby" => "rb",
        "yaml" | "yml" => "yaml",
        "markdown" | "md" => "md",
        other => other,
    };
    crate::review::highlight::colour_file(&format!("chat.{extension}"), source, 512)
}

#[cfg(test)]
mod tests;

//! The page's rhythm, after iA Writer: prose keeps a book's line length,
//! headings stay near the text's size and bind to what follows them, and
//! the space between blocks says how they relate. Sizes are in ems of the
//! chat's text, so the rhythm holds at every zoom.

use crate::chat::rich::{Block, Kind};
use pulldown_cmark::HeadingLevel;

/// Widest a line of prose runs at the default reading width, in ems: about
/// 65 to 70 characters, where reading is easiest. Code, tables, and
/// pictures take the whole column.
pub(super) const MEASURE: f32 = 34.;

/// Space above `block` when `previous` comes before it, in ems.
pub(super) fn space_before(previous: Option<&Block>, block: &Block) -> f32 {
    let Some(previous) = previous else {
        return 0.;
    };
    match (kind(previous), kind(block)) {
        // A heading belongs to what follows, so it keeps its distance from
        // what came before and stays close to its own text.
        (_, Some(Kind::Heading(level))) => match level {
            HeadingLevel::H1 | HeadingLevel::H2 => 1.4,
            _ => 1.1,
        },
        (Some(Kind::Heading(_)), _) => 0.35,
        // Items of one list read as one block.
        (Some(item), Some(next)) if is_item(item) && is_item(next) => 0.3,
        _ => 0.75,
    }
}

/// How much larger than the text a heading is set. Deeper levels are set
/// at the text's size; their weight sets them apart.
pub(super) fn heading_scale(level: HeadingLevel) -> f32 {
    match level {
        HeadingLevel::H1 => 1.35,
        HeadingLevel::H2 => 1.18,
        HeadingLevel::H3 => 1.05,
        _ => 1.,
    }
}

/// Whether `block` is prose, which keeps to [`MEASURE`].
pub(super) fn is_prose(block: &Block) -> bool {
    kind(block).is_some_and(|kind| kind != Kind::Rule)
}

fn kind(block: &Block) -> Option<Kind> {
    match block {
        Block::Paragraph { kind, .. } => Some(*kind),
        _ => None,
    }
}

fn is_item(kind: Kind) -> bool {
    matches!(kind, Kind::Bullet | Kind::Numbered(_))
}

#[cfg(test)]
mod tests;

//! What a font family choice applies to: one role, or all of them at once.
use crate::config::FontFace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontTarget {
    All,
    Face(FontFace),
}

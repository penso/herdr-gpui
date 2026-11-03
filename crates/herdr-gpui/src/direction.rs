//! Chrome that mirrors in a right-to-left language (docs/i18n.md, Phase 4).
//! GPUI lays everything out left to right, so the chrome names its sides
//! logically instead: the start is the left in English and the right in
//! Hebrew, and the end the other side. [`Logical`] gives every styled element
//! start and end padding, margins, insets, borders, corners, rows, and text
//! alignment; the functions here mirror a popover's anchor, a slide's
//! offset, and an icon that points somewhere.
//!
//! The direction is the shown locale's, read where there is no app context
//! as text is (`i18n::direction`), so a locale switch mirrors whatever is
//! built after it. The terminal grid and its content never mirror: panes,
//! splits, and tab strips are the daemon's geometry, and a terminal draws
//! its cells left to right whatever the interface reads.

use gpui::{AbsoluteLength, Anchor, DefiniteLength, Length, Styled, TextAlign};
pub(crate) use herdr_i18n::Direction;

/// The direction the chrome lays out in now.
pub(crate) fn current() -> Direction {
    crate::i18n::direction()
}

/// Whether the chrome lays out right to left now.
pub(crate) fn is_rtl() -> bool {
    current() == Direction::Rtl
}

/// Start and end sides for any styled element. Each picks the physical side
/// when it is called, so call them while building an element, not ahead.
pub(crate) trait Logical: Styled + Sized {
    /// Children from the start to the end: a row, reversed right to left.
    fn row(self) -> Self {
        let row = self.flex().flex_row();
        if is_rtl() {
            row.flex_row_reverse()
        } else {
            row
        }
    }

    /// Padding on the side a line of text starts.
    fn ps(self, length: impl Clone + Into<DefiniteLength>) -> Self {
        if is_rtl() {
            self.pr(length)
        } else {
            self.pl(length)
        }
    }

    /// Padding on the side a line of text ends.
    fn pe(self, length: impl Clone + Into<DefiniteLength>) -> Self {
        if is_rtl() {
            self.pl(length)
        } else {
            self.pr(length)
        }
    }

    fn ms(self, length: impl Clone + Into<Length>) -> Self {
        if is_rtl() {
            self.mr(length)
        } else {
            self.ml(length)
        }
    }

    fn me(self, length: impl Clone + Into<Length>) -> Self {
        if is_rtl() {
            self.ml(length)
        } else {
            self.mr(length)
        }
    }

    /// An absolutely placed element's distance from its container's start.
    fn inset_s(self, length: impl Clone + Into<Length>) -> Self {
        if is_rtl() {
            self.right(length)
        } else {
            self.left(length)
        }
    }

    /// An absolutely placed element's distance from its container's end.
    fn inset_e(self, length: impl Clone + Into<Length>) -> Self {
        if is_rtl() {
            self.left(length)
        } else {
            self.right(length)
        }
    }

    fn border_s(self, width: impl Clone + Into<AbsoluteLength>) -> Self {
        if is_rtl() {
            self.border_r(width)
        } else {
            self.border_l(width)
        }
    }

    fn border_e(self, width: impl Clone + Into<AbsoluteLength>) -> Self {
        if is_rtl() {
            self.border_l(width)
        } else {
            self.border_r(width)
        }
    }

    /// Rounds the two corners on the end side.
    fn rounded_e(self, radius: impl Clone + Into<AbsoluteLength>) -> Self {
        if is_rtl() {
            self.rounded_l(radius)
        } else {
            self.rounded_r(radius)
        }
    }

    /// Text set against the side it starts from.
    fn text_start(self) -> Self {
        self.text_align(text_start())
    }

    /// Text set against the side it ends on, as numbers in a column are.
    fn text_end(self) -> Self {
        self.text_align(match current() {
            Direction::Ltr => TextAlign::Right,
            Direction::Rtl => TextAlign::Left,
        })
    }
}

impl<E: Styled> Logical for E {}

/// The alignment of a line set from its start, for text painted by hand.
pub(crate) fn text_start() -> TextAlign {
    match current() {
        Direction::Ltr => TextAlign::Left,
        Direction::Rtl => TextAlign::Right,
    }
}

/// `anchor` as written for a left-to-right layout, mirrored when the chrome
/// runs right to left: a menu that hangs from its button's left corner
/// hangs from its right one.
pub(crate) fn anchor(anchor: Anchor) -> Anchor {
    if !is_rtl() {
        return anchor;
    }
    match anchor {
        Anchor::TopLeft => Anchor::TopRight,
        Anchor::TopRight => Anchor::TopLeft,
        Anchor::BottomLeft => Anchor::BottomRight,
        Anchor::BottomRight => Anchor::BottomLeft,
        Anchor::LeftCenter => Anchor::RightCenter,
        Anchor::RightCenter => Anchor::LeftCenter,
        centered @ (Anchor::TopCenter | Anchor::BottomCenter) => centered,
    }
}

/// A horizontal distance toward the end: positive to the right in a
/// left-to-right layout, to the left in a right-to-left one. A slide that
/// leaves toward the start moves by `toward_end(-width)`.
pub(crate) fn toward_end(x: f32) -> f32 {
    match current() {
        Direction::Ltr => x,
        Direction::Rtl => -x,
    }
}

/// Declares each icon or text arrow that points along the line once, with
/// its mirror, as `$function` and the `$all` table tests read.
macro_rules! mirrored {
    ($(#[$doc:meta])* $function:ident, $all:ident { $($one:literal <=> $other:literal,)+ }) => {
        $(#[$doc])*
        pub(crate) fn $function(shown: &'static str) -> &'static str {
            if !is_rtl() {
                return shown;
            }
            match shown {
                $(
                    $one => $other,
                    $other => $one,
                )+
                _ => shown,
            }
        }

        #[cfg(test)]
        pub(crate) const $all: &[(&str, &str)] = &[$(($one, $other),)+];
    };
}

mirrored! {
    /// `path`'s mirror image when the chrome runs right to left and the icon
    /// points along the line: a disclosure chevron, Back and Forward, a
    /// panel on one side. Every other icon is drawn as it is.
    icon, DIRECTIONAL_ICONS {
        // keep-sorted start
        "icons/arrow-left.svg" <=> "icons/arrow-right.svg",
        "icons/chevron-left.svg" <=> "icons/chevron-right.svg",
        "icons/panel-left.svg" <=> "icons/panel-right.svg",
        // keep-sorted end
    }
}

mirrored! {
    /// A text arrow that points along the line, such as a folded row's
    /// triangle or a breadcrumb's separator, mirrored right to left. Text
    /// the shaper lays out in a right-to-left run mirrors `›` itself; these
    /// are drawn alone, in a left-to-right paragraph.
    glyph, DIRECTIONAL_GLYPHS {
        // keep-sorted start
        "\u{203a}" <=> "\u{2039}",
        "\u{2192}" <=> "\u{2190}",
        "\u{25b8}" <=> "\u{25c2}",
        // keep-sorted end
    }
}

#[cfg(test)]
mod tests;

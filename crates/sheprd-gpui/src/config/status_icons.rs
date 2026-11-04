//! `status_icons` and `edge_rail`: how this client draws agent status. Herdr's
//! own `ui.status_indicators` is left to its terminal client; this app draws
//! only its own sets, so an unset key means Dots rather than Herdr's choice.

use serde::Deserialize;

/// Declares each set once with its config spelling and any older spellings
/// it still reads, so parsing, saving, and the settings list cannot drift.
macro_rules! status_icons {
    (
        $(#[doc = $first_doc:literal])* $first:ident => $first_name:literal,
        $($(#[doc = $doc:literal])* $set:ident => $name:literal $(| $alias:literal)*,)+
    ) => {
        #[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
        pub enum StatusIcons {
            $(#[doc = $first_doc])* #[default] #[serde(rename = $first_name)] $first,
            $($(#[doc = $doc])* #[serde(rename = $name $(, alias = $alias)*)] $set,)+
        }

        impl StatusIcons {
            /// Every set, in the order the settings window shows them.
            pub const ALL: &[Self] = &[Self::$first, $(Self::$set,)+];

            /// The config spelling, which deserializing reads back.
            pub fn as_str(self) -> &'static str {
                match self {
                    Self::$first => $first_name,
                    $(Self::$set => $name,)+
                }
            }
        }
    };
}

// Shown to users in this order, so not sorted. The first is the default.
status_icons! {
    /// Herdr's dots: filled, hollow for idle, with a turning arc for working.
    Dots => "dots",
    /// Filled discs with a cut-out mark, a crescent for idle, and a half disc
    /// turning in its ring for working.
    Solid => "solid",
    /// Outline marks of the same shapes, with an open arc spinning for working.
    Outline => "outline",
    /// Each mark on a tile tinted with its status color.
    Tiles => "tiles",
    /// After Superset's wave grid, every status drawn in the same 3×3 cells:
    /// the wave for working, a gap chasing round the frame when blocked.
    /// `wave-check` was a variant of it and now reads as it.
    Wave => "wave" | "wave-check",
    /// Every status a braille cell: a spinner while working, a cell that
    /// empties and fills when blocked, still cells otherwise.
    /// `dense-braille` was a variant of it and now reads as it.
    Braille => "braille" | "dense-braille",
    /// After Orca: a stepping ring while working, a question bubble when
    /// blocked, a ringed check when done.
    Orca => "orca",
}

/// `status_icons` as a file may hold it: a set, or the retired `rail`, which
/// drew outline icons with the edge rail and now reads as both.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(super) enum SavedIcons {
    #[serde(rename = "rail")]
    Rail,
    #[serde(untagged)]
    Set(StatusIcons),
}

impl SavedIcons {
    /// The set, and whether the rail is on when `edge_rail` does not say.
    pub(super) fn resolve(saved: Option<Self>) -> (StatusIcons, bool) {
        match saved {
            Some(Self::Rail) => (StatusIcons::Outline, true),
            Some(Self::Set(set)) => (set, false),
            None => (StatusIcons::default(), false),
        }
    }
}

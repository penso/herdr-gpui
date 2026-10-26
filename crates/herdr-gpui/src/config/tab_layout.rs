//! Named tab strip layouts.
use serde::Deserialize;

/// What goes wrong naming a tab layout.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unknown tab layout {0:?}")]
    Unknown(String),
}

/// Declares each layout once: its variant, the config value that selects it,
/// and how menus title it, in the order menus list them.
macro_rules! tab_layouts {
    ($($(#[$doc:meta])* $variant:ident => $name:literal, $label:literal;)+) => {
        /// How a group's tab strip draws its tabs. Each one is a
        /// [`crate::tab_layout::TabLayout`]; what a tab does is the same in all.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub enum TabLayoutMode {
            $($(#[$doc])* $variant,)+
        }

        impl TabLayoutMode {
            /// Every named layout, in the order menus list them.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// `ALL`'s names, for errors that list what a config may say.
            const NAMES: &'static [&'static str] = &[$($name,)+];

            /// The config value that selects it.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            /// How menus title it.
            pub const fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)+
                }
            }
        }
    };
}

tab_layouts! {
    /// Even cells divided by a rule, the chosen one washed in the accent.
    #[default]
    Classic => "classic", "Classic";
    /// Rounded pills on the strip, led by what runs in them, the status
    /// after the title giving way to the close button under the pointer.
    Pills => "pills", "Pills";
    /// Bare titles; the chosen tab stands in an outlined pill with its
    /// close button.
    Outline => "outline", "Outline";
    /// Flat titles without fills; the chosen tab carries a bar along its foot.
    Underline => "underline", "Underline";
    /// Browser-like: the chosen tab is a raised card joined to what it shows.
    Cards => "cards", "Cards";
    /// The chosen tab's body flares into the content below through curved
    /// shoulders and slides between tabs; the content sits in an inset card.
    Curved => "curved", "Curved";
    /// Icons only, with the status on each icon's corner; the chosen tab
    /// names itself.
    Compact => "compact", "Compact";
}

impl TryFrom<&str> for TabLayoutMode {
    type Error = Error;

    fn try_from(name: &str) -> Result<Self, Error> {
        Self::ALL
            .iter()
            .copied()
            .find(|layout| layout.name() == name)
            .ok_or_else(|| Error::Unknown(name.to_owned()))
    }
}

impl std::fmt::Display for TabLayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl<'de> Deserialize<'de> for TabLayoutMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::try_from(name.as_str())
            .map_err(|_| serde::de::Error::unknown_variant(&name, Self::NAMES))
    }
}

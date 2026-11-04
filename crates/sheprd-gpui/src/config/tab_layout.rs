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
            pub fn label(self) -> String {
                crate::i18n::message(self.message(), &[])
            }

            /// The message naming it.
            pub const fn message(self) -> &'static str {
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
    Classic => "classic", "tab-layout-classic";
    /// Rounded pills on the strip, led by what runs in them, the status
    /// after the title giving way to the close button under the pointer.
    Pills => "pills", "tab-layout-pills";
    /// Bare titles; the chosen tab stands in an outlined pill with its
    /// close button.
    Outline => "outline", "tab-layout-outline";
    /// Flat titles without fills; the chosen tab carries a bar along its foot.
    Underline => "underline", "tab-layout-underline";
    /// Browser-like: the chosen tab is a raised card joined to what it shows.
    Cards => "cards", "tab-layout-cards";
    /// The chosen tab's body flares into the content below through curved
    /// shoulders and slides between tabs; the content sits in an inset card.
    Curved => "curved", "tab-layout-curved";
    /// Icons only, with the status on each icon's corner; the chosen tab
    /// names itself.
    Compact => "compact", "tab-layout-compact";
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

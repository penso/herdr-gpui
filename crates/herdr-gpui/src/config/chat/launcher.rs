//! `[chat] launcher`: where a tab strip offers a one-click chat with each
//! installed agent. The strip's "+" opens a terminal tab in every placement;
//! the launcher sits beside it rather than replacing it.

use serde::Deserialize;

/// Declares each placement once: its variant, the config value that selects
/// it, and how Settings titles it, in the order Settings lists them.
macro_rules! chat_launchers {
    ($($(#[$doc:meta])* $variant:ident => $name:literal, $label:literal;)+) => {
        /// Where a tab strip draws its agent buttons.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub enum ChatLauncher {
            $($(#[$doc])* $variant,)+
        }

        impl ChatLauncher {
            /// Every placement, in the order Settings lists them.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// `ALL`'s names, for errors that list what a config may say.
            const NAMES: &'static [&'static str] = &[$($name,)+];

            /// The config value that selects it.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            /// How Settings titles it.
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

chat_launchers! {
    /// The agents slide out beside "+" while the pointer is over it.
    #[default]
    Plus => "plus", "chat-launcher-plus";
    /// The agents stand right after "+".
    Beside => "beside", "chat-launcher-beside";
    /// The agents lead the strip, before its tabs.
    Leading => "leading", "chat-launcher-leading";
    /// The agents stand at the strip's right end.
    Trailing => "trailing", "chat-launcher-trailing";
    /// One button for the agent last started at the right end; the others
    /// slide out beside it while the pointer is over it.
    Button => "button", "chat-launcher-button";
    /// No agent buttons; chats start from the "…" menu.
    Off => "off", "chat-launcher-off";
}

impl<'de> Deserialize<'de> for ChatLauncher {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::ALL
            .iter()
            .copied()
            .find(|launcher| launcher.name() == name)
            .ok_or_else(|| serde::de::Error::unknown_variant(&name, Self::NAMES))
    }
}

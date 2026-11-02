//! Optional behaviors under `[features]`. Each flag is declared once below,
//! which generates its config field, its `Feature` variant, and the messages
//! (`features.ftl`) Settings > General lists it with.
use serde::Deserialize;

macro_rules! features {
    ($($field:ident: $variant:ident, $message:literal, $description:literal;)+) => {
        /// Optional behaviors the config file turns on. Every flag is off by
        /// default, so a missing or empty `[features]` table is the shipped
        /// experience.
        #[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
        #[serde(default)]
        pub struct Features {
            $(#[doc = concat!("Described by the `", $description, "` message.")] pub $field: bool,)+
        }

        /// One `[features]` flag, for code that lists or edits them all.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Feature {
            $($variant,)+
        }

        impl Feature {
            /// Every flag, alphabetical by key, as Settings lists them.
            pub const ALL: &[Self] = &[$(Self::$variant,)+];

            /// The key under `[features]`.
            pub const fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($field),)+
                }
            }

            /// The id of the flag's row in Settings > General, which its
            /// search entry and debug selector share.
            pub const fn setting_id(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("general-feature-", stringify!($field)),)+
                }
            }

            /// The message naming the flag.
            pub const fn message(self) -> &'static str {
                match self {
                    $(Self::$variant => $message,)+
                }
            }

            /// The message describing the flag.
            pub const fn description_message(self) -> &'static str {
                match self {
                    $(Self::$variant => $description,)+
                }
            }
        }

        impl Features {
            pub fn enabled(&self, feature: Feature) -> bool {
                match feature {
                    $(Feature::$variant => self.$field,)+
                }
            }
        }
    };
}

// Settings lists the flags in this order, alphabetical by key, so additions
// from parallel branches land on different lines.
features! {
    // keep-sorted start
    acp_chats: AcpChats, "features-acp-chats",
        "features-acp-chats-description";
    qa_menu: QaMenu, "features-qa-menu",
        "features-qa-menu-description";
    sidebar_hover_menu: SidebarHoverMenu, "features-sidebar-hover-menu",
        "features-sidebar-hover-menu-description";
    // keep-sorted end
}

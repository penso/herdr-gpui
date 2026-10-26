//! Optional behaviors under `[features]`. Each flag is declared once below,
//! which generates its config field, its `Feature` variant, and the label and
//! description Settings > General lists it with.
use serde::Deserialize;

macro_rules! features {
    ($($field:ident: $variant:ident, $label:literal, $description:literal;)+) => {
        /// Optional behaviors the config file turns on. Every flag is off by
        /// default, so a missing or empty `[features]` table is the shipped
        /// experience.
        #[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
        #[serde(default)]
        pub struct Features {
            $(#[doc = $description] pub $field: bool,)+
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

            pub const fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)+
                }
            }

            pub const fn description(self) -> &'static str {
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
    acp_chats: AcpChats, "ACP chats",
        "Show herdr-acp chats as native chat views and offer new chats with the agents installed on the host. Off, chat panes keep their terminal and nothing looks for herdr-acp or agents.";
    qa_menu: QaMenu, "QA menu",
        "Add a QA menu for checking the app by hand: sample toasts and dialogs, update progress, sounds, a test notification, the badge, and slowed animations.";
    sidebar_hover_menu: SidebarHoverMenu, "Open space menus on hover",
        "Open a space's menu when the pointer rests on its sidebar row, without a right click. Moving the pointer anywhere but into that menu closes it again.";
    // keep-sorted end
}

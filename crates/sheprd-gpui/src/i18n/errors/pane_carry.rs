//! Why panes cannot move to another device, from `error-pane-carry.ftl`.

use super::Localize;
use crate::{i18n::fl, pane_menu::carry::Refusal};

impl Localize for Refusal {
    fn localize(&self) -> String {
        match self {
            Self::Busy => fl!("error-pane-carry-busy"),
            Self::Underway => fl!("error-pane-carry-underway"),
        }
    }
}

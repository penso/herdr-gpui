//! Teleport: `sheprd_remote::teleport`, plus its review and progress view.

// i18n: migrated
pub(crate) mod text;
mod ui;

pub(crate) use sheprd_remote::teleport::{snapshot::SessionKind, *};
pub(crate) use ui::{Follow, Teleport};

//! Teleport: `herdr_remote::teleport`, plus its review and progress view.

// i18n: migrated
pub(crate) mod text;
mod ui;

pub(crate) use herdr_remote::teleport::{snapshot::SessionKind, *};
pub(crate) use ui::{Follow, Teleport};

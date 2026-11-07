#![doc = include_str!("../README.md")]

mod error;
pub mod panes;
pub mod saved;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use error::{Error, Result};
pub use saved::{CoreTab, MAX_SAVED_BYTES, MAX_SAVED_TABS, SavedTabs};

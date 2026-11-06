//! `[worktrees]`: the hue linked Git worktrees are marked in, and where.
use super::parse_hex;
use crate::worktree_identity::{Error, Surface, Worktrees};
use serde::Deserialize;

/// The table as written. Unset keys keep the defaults: the theme's hue in
/// the sidebar, and no rule along the tab strip.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct WorktreeSettings {
    tint: Option<String>,
    apply_to: Option<Vec<Surface>>,
}

impl WorktreeSettings {
    pub(super) fn resolve(&self) -> Result<Worktrees, Error> {
        let tint = self
            .tint
            .as_deref()
            .map(|value| parse_hex(value).ok_or_else(|| Error::InvalidTint(value.to_owned())))
            .transpose()?;
        Ok(match &self.apply_to {
            Some(surfaces) => Worktrees::new(tint, surfaces),
            None => Worktrees {
                tint,
                ..Worktrees::default()
            },
        })
    }
}

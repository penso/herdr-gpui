//! `[worktrees]` setting failures, from `error-worktree-identity.ftl`.

use super::Localize;
use crate::{i18n::fl, worktree_identity::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::InvalidTint(tint) => fl!(
                "error-worktree-identity-invalid-tint",
                tint = format!("{tint:?}")
            ),
        }
    }
}

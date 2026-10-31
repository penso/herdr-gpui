//! Worktree script failures, from `error-worktree-scripts.ftl`.

use super::Localize;
use crate::{i18n::fl, worktree_scripts::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-worktree-scripts-busy"),
            Self::Checkout => fl!("error-worktree-scripts-checkout"),
            Self::Encoding(_) => fl!("error-worktree-scripts-encoding"),
            Self::NotGit => fl!("error-worktree-scripts-not-git"),
            Self::Nul => fl!("error-worktree-scripts-nul"),
            Self::Parse(error) => fl!("error-worktree-scripts-parse", error = error.to_string()),
            Self::Read(error) => fl!("error-worktree-scripts-read", error = error.to_string()),
            Self::Remote(error) => {
                fl!("error-worktree-scripts-remote", error = error.localize())
            }
            Self::Request(error) => error.localize(),
            Self::Response => fl!("error-worktree-scripts-response"),
            Self::Size { limit } => fl!("error-worktree-scripts-size", limit = *limit),
            Self::UnsupportedHost => fl!("error-worktree-scripts-unsupported-host"),
            // keep-sorted end
        }
    }
}

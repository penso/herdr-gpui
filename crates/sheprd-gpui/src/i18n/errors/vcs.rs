//! `sheprd_vcs`'s errors from `error-vcs.ftl`: Git commands, checkpoints,
//! and the code index. GitHub failures go through `sheprd_github`'s mapping,
//! and Git's own output stays a placeable.

use super::Localize;
use crate::i18n::fl;
use sheprd_vcs::{Error, blame, checkpoint, code_index, git, stage};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Blame(error) => error.localize(),
            Self::Checkpoint(error) => error.localize(),
            #[cfg(feature = "azure")]
            Self::CloudHostUnsupported => fl!("error-vcs-cloud-host-unsupported"),
            Self::CodeIndex(error) => error.localize(),
            Self::Git(error) => error.localize(),
            Self::GitHub(error) => error.localize(),
            Self::Stage(error) => error.localize(),
            // keep-sorted end
        }
    }
}

impl Localize for blame::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Contents(_) => fl!("error-vcs-blame-contents"),
            Self::Path => fl!("error-vcs-blame-path"),
            Self::Untracked => fl!("error-vcs-blame-untracked"),
            // keep-sorted end
        }
    }
}

impl Localize for stage::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Changed => fl!("error-vcs-stage-changed"),
            Self::NothingToStage => fl!("error-vcs-stage-nothing"),
            Self::Patch(_) => fl!("error-vcs-stage-patch"),
            // keep-sorted end
        }
    }
}

impl Localize for checkpoint::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Branch => fl!("error-vcs-checkpoint-branch"),
            Self::Busy => fl!("error-vcs-checkpoint-busy"),
            Self::Checkout => fl!("error-vcs-checkpoint-checkout"),
            Self::Missing => fl!("error-vcs-checkpoint-missing"),
            Self::Output => fl!("error-vcs-checkpoint-output"),
            Self::Script(_) => fl!("error-vcs-checkpoint-script"),
            Self::Thread(_) => fl!("error-vcs-checkpoint-thread"),
            Self::Unborn => fl!("error-vcs-checkpoint-unborn"),
            Self::Worker => fl!("error-vcs-checkpoint-worker"),
            // keep-sorted end
        }
    }
}

impl Localize for code_index::Error {
    fn localize(&self) -> String {
        match self {
            Self::Cancelled => fl!("error-vcs-code-index-cancelled"),
            Self::Root => fl!("error-vcs-code-index-root"),
        }
    }
}

impl Localize for git::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-vcs-git-busy"),
            Self::CommitMessage => fl!("error-vcs-git-commit-message"),
            Self::Failed { operation, details } => fl!(
                "error-vcs-git-failed",
                kind = failed_step(operation),
                operation = *operation,
                details = details.as_str()
            ),
            Self::NoCheckout => fl!("error-vcs-git-no-checkout"),
            Self::NothingStaged => fl!("error-vcs-git-nothing-staged"),
            Self::NothingToCommit => fl!("error-vcs-git-nothing-to-commit"),
            Self::Process { operation, .. } => fl!(
                "error-vcs-git-process",
                kind = match *operation {
                    "start the Git worker" => "start-worker", // i18n: data
                    _ => "other",
                },
                operation = *operation
            ),
            Self::PullRequestBase => fl!("error-vcs-git-pull-request-base"),
            Self::PullRequestTitle => fl!("error-vcs-git-pull-request-title"),
            Self::Worker => fl!("error-vcs-git-worker"),
            // keep-sorted end
        }
    }
}

/// The message key for a failed Git command's `operation`, which its
/// callers word in English; one not listed is shown as it is.
fn failed_step(operation: &str) -> &'static str {
    match operation {
        // keep-sorted start
        "check the file is tracked" => "check-tracked", // i18n: data
        "check uncommitted files" => "check-uncommitted", // i18n: data
        "check unpushed commits" => "check-unpushed",   // i18n: data
        "commit" => "commit",
        "count changes" => "count-changes", // i18n: data
        "count working tree changes" => "count-working-tree", // i18n: data
        "fetch the pull request branch" => "fetch-pull-request", // i18n: data
        "find the checkout" => "find-checkout", // i18n: data
        "find where the branch started" => "find-branch-start", // i18n: data
        "list attribute files" => "list-attribute-files", // i18n: data
        "list branches" => "list-branches", // i18n: data
        "list files" => "list-files",       // i18n: data
        "list untracked files" => "list-untracked", // i18n: data
        "list working tree changes" => "list-working-tree", // i18n: data
        "prepare the remote pull request or issue checkout" => "prepare-remote-checkout", // i18n: data
        "push" => "push",
        "read attributes" => "read-attributes", // i18n: data
        "read HEAD" => "read-head",             // i18n: data
        "read staged changes" => "read-staged", // i18n: data
        "read the commit subject" => "read-commit-subject", // i18n: data
        "read the new commit" => "read-new-commit", // i18n: data
        "read the remote's default branch" => "read-default-branch", // i18n: data
        "read unstaged changes" => "read-unstaged", // i18n: data
        "read who changed each line" => "read-blame", // i18n: data
        "read working tree changes" => "read-working-tree-changes", // i18n: data
        "read working tree state" => "read-working-tree-state", // i18n: data
        "stage changes" => "stage",             // i18n: data
        "stage the file" => "stage-file",       // i18n: data
        "stage the hunk" => "stage-hunk",       // i18n: data
        // keep-sorted end
        _ => "other",
    }
}

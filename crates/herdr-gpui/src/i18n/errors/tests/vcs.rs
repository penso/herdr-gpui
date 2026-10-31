use super::*;
use herdr_vcs::{Error, checkpoint, code_index, git};
use std::io;

fn io() -> io::Error {
    io::Error::other("pipe closed")
}

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Checkpoint(checkpoint::Error::Busy),
        #[cfg(feature = "azure")]
        Error::CloudHostUnsupported,
        Error::CodeIndex(code_index::Error::Root),
        Error::Git(git::Error::Busy),
        Error::GitHub(herdr_github::Error::Query),
    ]);
}

#[test]
fn every_checkpoint_variant_has_a_message() {
    assert_localized(&[
        checkpoint::Error::Branch,
        checkpoint::Error::Busy,
        checkpoint::Error::Checkout,
        checkpoint::Error::Missing,
        checkpoint::Error::Output,
        checkpoint::Error::Script(herdr_client::Error::Disconnected),
        checkpoint::Error::Thread(io()),
        checkpoint::Error::Unborn,
        checkpoint::Error::Worker,
    ]);
}

#[test]
fn every_code_index_variant_has_a_message() {
    assert_localized(&[code_index::Error::Cancelled, code_index::Error::Root]);
}

#[test]
fn every_git_variant_has_a_message() {
    let mut errors = vec![
        git::Error::Busy,
        git::Error::CommitMessage,
        git::Error::NoCheckout,
        git::Error::NothingToCommit,
        git::Error::PullRequestBase,
        git::Error::PullRequestTitle,
        git::Error::Worker,
    ];
    errors.extend(
        [
            "check uncommitted files",
            "check unpushed commits",
            "commit",
            "count changes",
            "count working tree changes",
            "fetch the pull request branch",
            "find the checkout",
            "find where the branch started",
            "list attribute files",
            "list branches",
            "list files",
            "list untracked files",
            "list working tree changes",
            "prepare the remote pull request or issue checkout",
            "push",
            "read HEAD",
            "read attributes",
            "read the commit subject",
            "read the new commit",
            "read the remote's default branch",
            "read working tree changes",
            "read working tree state",
            "stage changes",
            "rebase onto the moon",
        ]
        .map(|operation| git::Error::Failed {
            operation,
            details: "fatal: not a git repository".into(),
        }),
    );
    errors.extend(["start the Git worker", "start the moon"].map(|operation| {
        git::Error::Process {
            operation,
            source: io(),
        }
    }));
    assert_localized(&errors);
}

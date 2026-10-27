use super::super::{Target, fetch};
use super::*;
use crate::pull_request::{Input, Origin as Host};

#[test]
fn cancelled_remote_preparation_never_runs_ssh_or_reads_a_local_checkout() {
    let target = Target {
        host: Host::Ssh("unused.invalid".into()),
        input: Input {
            checkout: Some("/must-not-be-read-locally".into()),
            repo_key: Some("/remote/repository/.git".into()),
            branch: "main".into(),
        },
    };
    let item = &parse(&response(), &origin(), Kind::PullRequest).unwrap()[0];
    assert!(matches!(
        fetch::fetch_branch(&target, item, &|| true),
        Err(Error::GitHub(herdr_github::Error::PullRequest(
            herdr_github::pull_request::Error::Cancelled
        )))
    ));

    let target = Target {
        input: Input {
            repo_key: None,
            ..target.input
        },
        ..target
    };
    assert!(matches!(
        fetch::fetch_branch(&target, item, &|| false),
        Err(Error::GitHub(herdr_github::Error::PullRequest(
            herdr_github::pull_request::Error::Metadata
        )))
    ));
}

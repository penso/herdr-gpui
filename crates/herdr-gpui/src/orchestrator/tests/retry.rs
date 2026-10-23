use super::*;
use crate::orchestrator::service::retry_wait;
use std::time::Duration;

const FIVE_MINUTES: Duration = Duration::from_secs(300);
const HALF_HOUR: Duration = Duration::from_secs(1800);
const HOUR: Duration = Duration::from_secs(3600);

fn github(error: crate::Error) -> Error {
    Error::Github(Box::new(error))
}

#[test]
fn a_beads_failure_ignores_a_pause_github_left_behind() {
    assert_eq!(retry_wait(&Error::BeadsMissing, Some(HOUR)), FIVE_MINUTES);
    assert_eq!(retry_wait(&Error::BeadsMissing, None), FIVE_MINUTES);
}

#[test]
fn a_github_failure_waits_as_long_as_github_asked() {
    let limited = github(crate::Error::GitHub(crate::github::Error::RateLimit {
        reset: None,
    }));
    assert_eq!(retry_wait(&limited, Some(HOUR)), HOUR);
    assert_eq!(retry_wait(&limited, None), FIVE_MINUTES);
    let hidden = github(crate::Error::GitHub(
        crate::github::Error::RepositoryHidden {
            repository: "acme/hidden".into(),
            app_token: true,
        },
    ));
    assert_eq!(retry_wait(&hidden, None), HALF_HOUR);
    assert_eq!(retry_wait(&hidden, Some(FIVE_MINUTES)), HALF_HOUR);
    assert_eq!(retry_wait(&hidden, Some(HOUR)), HOUR);
}

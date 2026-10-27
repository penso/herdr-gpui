//! A failed sync's banner names its backend once.
use crate::orchestrator::{Error, Provider, view::inbox::failure_text};

#[test]
fn a_failure_names_its_backend_once() {
    assert_eq!(
        failure_text(Provider::Beads, &Error::BeadsMissing),
        "Beads is not installed on this host (bd was not found)"
    );
    assert_eq!(
        failure_text(Provider::Github, &"rate limited"),
        "GitHub: rate limited"
    );
}

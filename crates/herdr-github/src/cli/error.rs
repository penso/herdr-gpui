//! Why the GitHub CLI's sign-in could not be read.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("GitHub CLI (gh) was not found. Install it and run `gh auth login`, or sign in here.")]
    Missing,
    #[error("Could not run the GitHub CLI (gh).")]
    Process(#[source] Box<crate::Error>),
    #[error(
        "GitHub rejected the GitHub CLI's token. Run `gh auth login` to renew it, then try again."
    )]
    Rejected,
    #[error("GitHub CLI is not signed in to github.com. Run `gh auth login`, then try again.")]
    SignedOut,
    #[error("GitHub CLI did not answer within 10 seconds. Check `gh auth status`, then try again.")]
    Timeout,
    // keep-sorted end
}

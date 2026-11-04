//! Deleting a workspace at Coder: wait out a build already running, queue a
//! delete build, and poll until Coder no longer has the workspace. A workspace
//! that is already gone, or already deleting, is not deleted twice, so a
//! destroy interrupted by closing Settings can simply run again. A delete that
//! failed or was canceled before this attempt is queued again, so a destroy can
//! be retried once the problem at Coder is fixed.

use super::{
    Error, Result, Settings,
    api::{self, BUILD_TIMEOUT, BuildStatus, Client, Transition, Workspace},
    store,
};
use secrecy::SecretString;
use std::time::Instant;

/// How far a destroy has come, for the progress bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Another build is still running; Coder accepts one build at a time.
    Waiting,
    /// The delete build is queued behind Coder's provisioners.
    Queued,
    /// Coder is tearing the workspace's resources down.
    Deleting,
}

/// What the latest build means for a destroy.
enum State {
    Gone,
    /// The latest build is a delete that has not finished.
    Deleting(Stage),
    /// A start or stop build has not settled yet.
    Busy,
    /// Nothing is running, so a delete build can be queued.
    Idle,
}

/// `queued` says whether this attempt queued a delete build itself: only that
/// build's failure ends the attempt, while an earlier one is retried.
fn state(workspace: &Workspace, queued: bool) -> Result<State> {
    let build = &workspace.latest_build;
    if build.status == BuildStatus::Deleted {
        return Ok(State::Gone);
    }
    if build.transition == Transition::Delete {
        if let Some(reason) = build.failure() {
            return if queued {
                Err(Error::Workspace(reason))
            } else {
                Ok(State::Idle)
            };
        }
        return Ok(State::Deleting(match build.status {
            BuildStatus::Pending => Stage::Queued,
            _ => Stage::Deleting,
        }));
    }
    Ok(match build.status {
        BuildStatus::Pending
        | BuildStatus::Starting
        | BuildStatus::Stopping
        | BuildStatus::Canceling
        | BuildStatus::Deleting => State::Busy,
        _ => State::Idle,
    })
}

/// Delete workspace `id` and return once Coder no longer has it. Each poll
/// asks `tokens` for a current token, like `api::wait_ready`.
pub fn destroy(
    settings: &Settings,
    tokens: &impl Fn(bool) -> Result<SecretString>,
    id: &str,
    cancelled: impl Fn() -> bool,
    mut stage: impl FnMut(Stage),
) -> Result<()> {
    let deadline = Instant::now() + BUILD_TIMEOUT;
    let mut queued = false;
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let workspace =
            match store::with_token(tokens, |token| Client::new(settings, token).workspace(id)) {
                Err(Error::Deleted) => return Ok(()),
                result => result?,
            };
        match state(&workspace, queued)? {
            State::Gone => return Ok(()),
            State::Deleting(current) => stage(current),
            State::Busy => stage(Stage::Waiting),
            State::Idle => {
                stage(Stage::Queued);
                match store::with_token(tokens, |token| {
                    Client::new(settings, token).transition(id, Transition::Delete)
                }) {
                    Err(Error::Status(super::Status {
                        code: 404 | 410, ..
                    })) => return Ok(()),
                    result => result?,
                }
                queued = true;
            }
        }
        if Instant::now() >= deadline {
            return Err(Error::DeleteTimeout);
        }
        api::pause(&cancelled)?;
    }
}

#[cfg(test)]
mod tests;

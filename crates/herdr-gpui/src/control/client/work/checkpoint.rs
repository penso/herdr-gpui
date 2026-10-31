//! `checkpoint list|create|diff`, run in this process for the checkout of
//! the current directory with the app's own scripts, so Herdr UI need not
//! run; `checkpoint restore` opens the app's dialog, where the user confirms.
use super::{
    super::{Context, Output},
    rejected,
};
use crate::{
    checkpoint::{Checkout, Checkpoint, direct},
    cli::control::CheckpointCommand,
    control::{ErrorCode, protocol::Request, protocol::work::WorkOp},
};
use serde::Serialize;
use std::fmt::Write as _;

/// One checkpoint as `checkpoint list --json` prints it.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct CheckpointInfo {
    pub id: String,
    /// Unix seconds.
    pub created: i64,
    pub label: String,
    pub branch: Option<String>,
    pub files: u64,
    pub additions: u64,
    pub deletions: u64,
}

impl From<Checkpoint> for CheckpointInfo {
    fn from(checkpoint: Checkpoint) -> Self {
        Self {
            id: checkpoint.id,
            created: checkpoint.created,
            label: checkpoint.label,
            branch: checkpoint.branch,
            files: checkpoint.diff.files,
            additions: checkpoint.diff.additions,
            deletions: checkpoint.diff.deletions,
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// One line per checkpoint, newest first.
pub(super) fn describe(checkpoints: &[CheckpointInfo], branch: &str, now: i64) -> String {
    if checkpoints.is_empty() {
        return format!("{branch} has no checkpoints");
    }
    let mut text = String::new();
    for checkpoint in checkpoints {
        let _ = writeln!(
            text,
            "{}  {:<12}  {}  ({} file{}, +{} -{})",
            checkpoint.id,
            crate::ago::english(crate::ago::since(checkpoint.created, now)),
            checkpoint.label,
            checkpoint.files,
            if checkpoint.files == 1 { "" } else { "s" },
            checkpoint.additions,
            checkpoint.deletions,
        );
    }
    text.trim_end().to_owned()
}

/// The checkout of the current directory.
fn here() -> crate::Result<Checkout> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(crate::control::Error::Rejected {
            code: ErrorCode::Unsupported,
            message: "Checkpoints run POSIX shell scripts, which this platform does not have"
                .into(),
        }
        .into());
    }
    let dir = std::env::current_dir().map_err(|error| rejected(crate::Error::LocalGit(error)))?;
    super::here::checkout(&dir).map_err(rejected)
}

pub(in crate::control::client) fn checkpoint(
    command: CheckpointCommand,
    context: &Context,
) -> crate::Result<Output> {
    match command {
        CheckpointCommand::Restore { id } => context.act(Request::Work, WorkOp::Restore { id }),
        CheckpointCommand::List => list(&here()?),
        CheckpointCommand::Create { label } => create(&here()?, label),
        CheckpointCommand::Diff { from, to } => diff(&here()?, &from, to.as_deref()),
    }
}

fn list(checkout: &Checkout) -> crate::Result<Output> {
    let checkpoints: Vec<CheckpointInfo> = direct::list(checkout, direct::here)
        .map_err(rejected)?
        .into_iter()
        .map(CheckpointInfo::from)
        .collect();
    Output::data(
        describe(&checkpoints, &checkout.branch, now()),
        &serde_json::json!({
            "repository": checkout.repo_key,
            "branch": checkout.branch,
            "checkpoints": checkpoints,
        }),
    )
}

fn create(checkout: &Checkout, label: Option<String>) -> crate::Result<Output> {
    let label = label.unwrap_or_else(|| "Taken from the command line".to_owned());
    let id = direct::create(checkout, &label, direct::here).map_err(rejected)?;
    let text = match &id {
        Some(id) => format!("Took checkpoint {id} of {}", checkout.branch),
        None => format!(
            "Nothing changed in {} since its latest checkpoint",
            checkout.branch
        ),
    };
    Output::data(text, &serde_json::json!({ "id": id }))
}

/// The patch exactly as Git printed it, so `checkpoint diff A > p` applies
/// with `git apply p`; JSON carries it as text, with bytes that are not
/// UTF-8 replaced.
fn diff(checkout: &Checkout, from: &str, to: Option<&str>) -> crate::Result<Output> {
    let patch = direct::diff(checkout, from, to, direct::here_bytes).map_err(rejected)?;
    let json = serde_json::json!({
        "from": from,
        "to": to,
        "patch": String::from_utf8_lossy(&patch),
    });
    // No changes: a terminal is told so, and a pipe gets the empty patch.
    let mut output = Output::bytes(patch, &json)?;
    if output.text.is_empty() {
        output.text = "No changes".into();
    }
    Ok(output)
}

#[cfg(test)]
mod tests;

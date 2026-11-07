//! `GET /api/v2/workspaces/{workspace}/changes` and its neighbours: a
//! workspace's changed files, one file's diff, and its pull request's checks.
//! All read-only, and all worked out by Sheprd (`control::changes`); this
//! only puts the phone's question to it and hands back the answer.

mod error;
mod mailbox;

pub(crate) use error::Error;
pub(crate) use mailbox::Mailbox;

use crate::{
    control::changes::{
        Answer, CheckLog, Checks, Failure, FileDiff, FileList, MAX_PATH, Scope, Wanted,
    },
    web::{ApiError, Device, Shared},
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use std::time::{Duration, Instant};

/// How long a phone's question waits for Sheprd: a large repository's
/// listing takes a while, and the phone allows a little longer.
const ANSWER_WAIT: Duration = Duration::from_secs(25);

#[derive(Deserialize)]
pub(crate) struct FilesQuery {
    #[serde(default)]
    scope: Scope,
}

#[derive(Deserialize)]
pub(crate) struct DiffQuery {
    #[serde(default)]
    scope: Scope,
    path: String,
}

/// Puts `wanted` to Sheprd and waits for its answer. A workspace this
/// daemon does not list is refused here, so Sheprd is only asked about ones
/// the phone could see.
async fn ask(state: &Shared, workspace: String, wanted: Wanted) -> Result<Answer, ApiError> {
    let boot = {
        let layout = state.hub.layout();
        layout.workspace(&workspace).ok_or(ApiError::NotFound)?;
        layout.boot.clone()
    };
    let mut pending = state
        .changes
        .ask(boot, workspace, wanted, Instant::now())
        .map_err(ApiError::from)?;
    match tokio::time::timeout(ANSWER_WAIT, pending.answer()).await {
        Ok(Some(Answer::Failed(failure))) => Err(Error::Failed(failure).into()),
        Ok(Some(answer)) => Ok(answer),
        Ok(None) | Err(_) => Err(Error::TimedOut.into()),
    }
}

/// An answer that does not fit its question: never one from this Sheprd.
fn mismatch() -> ApiError {
    Error::Failed(Failure::Unreadable).into()
}

pub(crate) async fn files(
    State(state): State<Shared>,
    _device: Device,
    Path(workspace): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<FileList>, ApiError> {
    let wanted = Wanted::Files { scope: query.scope };
    match ask(&state, workspace, wanted).await? {
        Answer::Files(list) => Ok(Json(list)),
        _ => Err(mismatch()),
    }
}

pub(crate) async fn diff(
    State(state): State<Shared>,
    _device: Device,
    Path(workspace): Path<String>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<FileDiff>, ApiError> {
    if query.path.is_empty() || query.path.len() > MAX_PATH {
        return Err(ApiError::Invalid("that is not a changed file"));
    }
    let wanted = Wanted::Diff {
        scope: query.scope,
        path: query.path,
    };
    match ask(&state, workspace, wanted).await? {
        Answer::Diff(diff) => Ok(Json(diff)),
        _ => Err(mismatch()),
    }
}

pub(crate) async fn checks(
    State(state): State<Shared>,
    _device: Device,
    Path(workspace): Path<String>,
) -> Result<Json<Checks>, ApiError> {
    match ask(&state, workspace, Wanted::Checks).await? {
        Answer::Checks(checks) => Ok(Json(checks)),
        _ => Err(mismatch()),
    }
}

pub(crate) async fn check_log(
    State(state): State<Shared>,
    _device: Device,
    Path((workspace, job)): Path<(String, u64)>,
) -> Result<Json<CheckLog>, ApiError> {
    match ask(&state, workspace, Wanted::CheckLog { job }).await? {
        Answer::CheckLog(log) => Ok(Json(log)),
        _ => Err(mismatch()),
    }
}

//! The HTML fragments the shell swaps into its view, each behind a session.

use super::{
    ApiError, Device, Shared,
    render::{self, Folders, Lists, now_ms},
};
use crate::hub::{Layout, Summary};
use axum::{
    extract::{Path, Query, State},
    response::Html,
};
use maud::Markup;
use serde::Deserialize;
use std::{collections::HashMap, path::Path as FsPath};

/// The most folders one project root lists.
const MAX_FOLDERS: usize = 60;
/// The most project roots the form lists.
const MAX_ROOTS: usize = 8;

/// Every chat's row now.
pub(super) fn summaries(state: &Shared, layout: &Layout) -> HashMap<String, Summary> {
    layout
        .chats()
        .filter_map(|chat| {
            state
                .hub
                .chat(&chat.pane)
                .map(|link| (chat.pane.clone(), link.summary()))
        })
        .collect()
}

/// Home or the board, rendered from the state now; also the number of
/// questions waiting, for the app badge.
pub(super) fn lists(state: &Shared, board: bool) -> (Markup, usize) {
    let layout = state.hub.layout();
    let summaries = summaries(state, &layout);
    let lists = Lists {
        computer: &state.computer,
        layout: &layout,
        summaries: &summaries,
        now_ms: now_ms(),
    };
    let markup = if board {
        render::board(&lists)
    } else {
        render::home(&lists)
    };
    (markup, lists.waiting())
}

pub(super) async fn home(State(state): State<Shared>, _device: Device) -> Html<String> {
    Html(lists(&state, false).0.into_string())
}

pub(super) async fn board(State(state): State<Shared>, _device: Device) -> Html<String> {
    Html(lists(&state, true).0.into_string())
}

pub(super) async fn chat(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
) -> Result<Html<String>, ApiError> {
    let layout = state.hub.layout();
    let (workspace, row) = layout.chat(&pane).ok_or(ApiError::NotFound)?;
    let link = state.hub.chat(&pane).ok_or(ApiError::NotFound)?;
    Ok(Html(
        render::chat(workspace, row, &link.view(), now_ms()).into_string(),
    ))
}

pub(super) async fn history(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
) -> Result<Html<String>, ApiError> {
    let link = state.hub.chat(&pane).ok_or(ApiError::NotFound)?;
    Ok(Html(render::chat_history(&link.view()).into_string()))
}

pub(super) async fn new_workspace(
    State(state): State<Shared>,
    _device: Device,
) -> Result<Html<String>, ApiError> {
    let layout = state.hub.layout();
    let folders = tokio::task::spawn_blocking(move || folders(&layout))
        .await
        .map_err(|_| ApiError::Folder)?;
    Ok(Html(render::new_workspace(&folders).into_string()))
}

#[derive(Deserialize)]
pub(super) struct NewChatQuery {
    workspace: String,
}

pub(super) async fn new_chat(
    State(state): State<Shared>,
    _device: Device,
    Query(query): Query<NewChatQuery>,
) -> Result<Html<String>, ApiError> {
    let layout = state.hub.layout();
    let workspace = layout
        .workspace(&query.workspace)
        .ok_or(ApiError::NotFound)?;
    Ok(Html(render::new_chat(workspace).into_string()))
}

pub(super) async fn settings(State(state): State<Shared>, _device: Device) -> Html<String> {
    Html(render::devices(&state.computer, &state.origins(), &state.fingerprint).into_string())
}

/// The folders a new workspace may start in: those workspaces use, and the
/// folders beside them (their parents are the project roots). Blocking:
/// it reads directories.
pub(crate) fn folders(layout: &Layout) -> Folders {
    let mut recent: Vec<String> = Vec::new();
    for workspace in &layout.workspaces {
        if !workspace.cwd.is_empty() && !recent.contains(&workspace.cwd) {
            recent.push(workspace.cwd.clone());
        }
    }
    let mut roots: Vec<String> = Vec::new();
    for folder in &recent {
        if let Some(parent) = FsPath::new(folder)
            .parent()
            .and_then(|parent| parent.to_str())
            && !parent.is_empty()
            && parent != "/"
            && !roots.iter().any(|root| root == parent)
            && roots.len() < MAX_ROOTS
        {
            roots.push(parent.to_owned());
        }
    }
    let roots = roots
        .into_iter()
        .map(|root| {
            let children = children(FsPath::new(&root));
            (root, children)
        })
        .filter(|(_, children)| !children.is_empty())
        .collect();
    Folders { recent, roots }
}

/// The visible folders directly in `root`, sorted, at most [`MAX_FOLDERS`].
fn children(root: &FsPath) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut folders: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|entry| entry.path().to_str().map(str::to_owned))
        .take(MAX_FOLDERS * 4)
        .collect();
    folders.sort();
    folders.truncate(MAX_FOLDERS);
    folders
}

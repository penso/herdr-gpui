//! Terminal tabs: the page for one pane, its screen as the stream sends it,
//! and what the phone types or asks of Herdr. Everything goes through
//! Herdr's public API, which neither focuses nor resizes anything, so a
//! phone watching or typing leaves the desktop's layout alone. Only panes
//! of terminal tabs in the layout are reachable, never a chat's pane.

use super::{ApiError, Device, Shared, render};
use crate::hub::{ApiMethod, Layout};
use axum::{
    Json,
    extract::{Path, State},
    response::Html,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// The most keys one request sends.
const MAX_KEYS: usize = 64;
/// The longest paste one request sends.
const MAX_TEXT: usize = 64 * 1024;

/// Key names Herdr's `pane.send_keys` reads besides single characters.
const NAMED_KEYS: &[&str] = &[
    // keep-sorted start
    "backspace",
    "backtick",
    "down",
    "enter",
    "esc",
    "left",
    "minus",
    "plus",
    "right",
    "slash",
    "space",
    "tab",
    "up",
    // keep-sorted end
];
const MODIFIERS: &[&str] = &["alt", "ctrl", "shift"];

/// Whether `key` is one key in Herdr's syntax: modifiers joined by `+`,
/// then a named key, a function key, or one printable character.
pub(crate) fn valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 32 {
        return false;
    }
    let mut parts: Vec<&str> = key.split('+').collect();
    let Some(last) = parts.pop() else {
        return false;
    };
    let modifiers_ok = parts.iter().all(|part| MODIFIERS.contains(part));
    let mut chars = last.chars();
    let single = matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_control() && !c.is_whitespace());
    let function = last
        .strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=12).contains(&number));
    modifiers_ok && (single || function || NAMED_KEYS.contains(&last))
}

/// The terminal pane `pane` names in `layout`, or not found.
fn known(layout: &Layout, pane: &str) -> Result<(), ApiError> {
    layout.terminal(pane).map(|_| ()).ok_or(ApiError::NotFound)
}

/// The pane's visible screen, as text with SGR colors.
pub(crate) async fn read_screen(state: &Shared, pane: &str) -> Result<String, ApiError> {
    let read = state
        .hub
        .call(
            ApiMethod::PaneRead,
            json!({"pane_id": pane, "source": "visible", "format": "ansi"}),
        )
        .await?;
    Ok(read["read"]["text"].as_str().unwrap_or_default().to_owned())
}

pub(super) async fn fragment(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
) -> Result<Html<String>, ApiError> {
    // Checked before reading, so a chat's pane is never read.
    known(&state.hub.layout(), &pane)?;
    let text = read_screen(&state, &pane).await?;
    let layout = state.hub.layout();
    let (workspace, terminal) = layout.terminal(&pane).ok_or(ApiError::NotFound)?;
    let row = terminal
        .panes
        .iter()
        .find(|candidate| candidate.pane == pane)
        .ok_or(ApiError::NotFound)?;
    let screen = render::screen(&text);
    Ok(Html(
        render::terminal(workspace, terminal, row, &screen).into_string(),
    ))
}

#[derive(Deserialize)]
pub(super) struct Keys {
    keys: Vec<String>,
}

pub(super) async fn keys(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(request): Json<Keys>,
) -> Result<Json<Value>, ApiError> {
    known(&state.hub.layout(), &pane)?;
    if request.keys.is_empty()
        || request.keys.len() > MAX_KEYS
        || !request.keys.iter().all(|key| valid_key(key))
    {
        return Err(ApiError::Invalid("those keys cannot be sent"));
    }
    state
        .hub
        .call(
            ApiMethod::PaneSendKeys,
            json!({"pane_id": pane, "keys": request.keys}),
        )
        .await?;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
pub(super) struct Paste {
    text: String,
}

/// Text pasted as a whole: Herdr wraps it in a bracketed paste when the
/// program asked for one.
pub(super) async fn paste(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(request): Json<Paste>,
) -> Result<Json<Value>, ApiError> {
    known(&state.hub.layout(), &pane)?;
    if request.text.is_empty() || request.text.len() > MAX_TEXT {
        return Err(ApiError::Invalid("that text cannot be pasted"));
    }
    state
        .hub
        .call(
            ApiMethod::PaneSendText,
            json!({"pane_id": pane, "text": request.text}),
        )
        .await?;
    Ok(Json(json!({})))
}

/// What the action sheet asks of Herdr.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    // keep-sorted start
    ClosePane,
    CloseTab,
    NewTab,
    SplitDown,
    SplitRight,
    Zoom,
    // keep-sorted end
}

#[derive(Deserialize)]
pub(super) struct ActionRequest {
    action: Action,
}

/// The Herdr method and parameters for `action` on `pane` in `tab`.
pub(crate) fn action_call(
    action: Action,
    pane: &str,
    tab: &str,
    workspace: &str,
) -> (ApiMethod, Value) {
    match action {
        Action::SplitRight | Action::SplitDown => (
            ApiMethod::PaneSplit,
            json!({
                "target_pane_id": pane,
                "direction": if action == Action::SplitRight { "right" } else { "down" },
                "focus": false,
            }),
        ),
        Action::Zoom => (
            ApiMethod::PaneZoom,
            json!({"pane_id": pane, "mode": "toggle"}),
        ),
        Action::ClosePane => (ApiMethod::PaneClose, json!({"pane_id": pane})),
        Action::NewTab => (
            ApiMethod::TabCreate,
            json!({"workspace_id": workspace, "focus": false}),
        ),
        Action::CloseTab => (ApiMethod::TabClose, json!({"tab_id": tab})),
    }
}

/// Runs an action from the sheet; answers where the phone goes next: the
/// new pane or tab, or home when the pane is gone.
pub(super) async fn action(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(request): Json<ActionRequest>,
) -> Result<Json<Value>, ApiError> {
    let (tab, workspace) = {
        let layout = state.hub.layout();
        let (workspace, terminal) = layout.terminal(&pane).ok_or(ApiError::NotFound)?;
        (terminal.tab.clone(), workspace.id.clone())
    };
    let (method, params) = action_call(request.action, &pane, &tab, &workspace);
    let result = state.hub.call(method, params).await?;
    let created = result["pane"]["pane_id"]
        .as_str()
        .or_else(|| result["root_pane"]["pane_id"].as_str())
        .filter(|id| !id.is_empty());
    let location = match (request.action, created) {
        (Action::SplitRight | Action::SplitDown | Action::NewTab, Some(created)) => {
            format!("/term/{}", render::urlencode(created))
        }
        (Action::ClosePane | Action::CloseTab, _) => "/".to_owned(),
        _ => format!("/term/{}", render::urlencode(&pane)),
    };
    Ok(Json(json!({ "location": location })))
}

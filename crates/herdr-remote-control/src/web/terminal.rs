//! Terminal tabs: the page for one pane, its screen as the stream sends it,
//! and what the phone types or asks of Herdr. Everything goes through
//! Herdr's public API, which neither focuses nor resizes anything, so a
//! phone watching or typing leaves the desktop's layout alone. Only panes
//! of the layout's terminal tabs and chats are reachable.

use super::{ApiError, Device, Shared, render, render::now_ms};
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
    state.typed.typed(&pane);
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
    state.typed.typed(&pane);
    Ok(Json(json!({})))
}

/// A picture pasted into a terminal: stored like a chat's upload, then its
/// path typed into the pane as a terminal file drop would, so a program such
/// as Claude Code attaches it. The phone never names the path; this computer
/// chooses it.
pub(super) async fn upload(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    bytes: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    known(&state.hub.layout(), &pane)?;
    let id = crate::auth::random_id()?;
    let stored = state.clone();
    let path = tokio::task::spawn_blocking(move || {
        let now = now_ms();
        let id = stored.uploads.store(id, &bytes, now)?;
        stored.uploads.path(&id, now)
    })
    .await
    .map_err(|_| ApiError::Invalid("the upload was interrupted"))??;
    state
        .hub
        .call(
            ApiMethod::PaneSendText,
            json!({"pane_id": pane, "text": dropped_path(&path)}),
        )
        .await?;
    state.typed.typed(&pane);
    Ok(Json(json!({})))
}

/// `path` as a terminal types a dropped file: shell characters escaped with
/// a backslash, and a space after it so typing goes on.
pub(crate) fn dropped_path(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    if cfg!(windows) {
        return format!("\"{path}\" ");
    }
    let mut text = String::with_capacity(path.len() + 8);
    for character in path.chars() {
        if !(character.is_alphanumeric()
            || matches!(character, '/' | '.' | '_' | '-' | '+' | ',' | ':' | '@'))
        {
            text.push('\\');
        }
        text.push(character);
    }
    text.push(' ');
    text
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
/// new pane or tab, or home when the pane is gone. A chat's pane is a
/// Herdr pane too, so the phone closes an agent the same way.
pub(super) async fn action(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(request): Json<ActionRequest>,
) -> Result<Json<Value>, ApiError> {
    let (tab, workspace) = {
        let layout = state.hub.layout();
        layout
            .terminal(&pane)
            .map(|(workspace, terminal)| (terminal.tab.clone(), workspace.id.clone()))
            .or_else(|| {
                layout
                    .chat(&pane)
                    .map(|(workspace, chat)| (chat.tab.clone(), workspace.id.clone()))
            })
            .ok_or(ApiError::NotFound)?
    };
    let (method, params) = action_call(request.action, &pane, &tab, &workspace);
    let result = state.hub.call(method, params).await?;
    let created = result["pane"]["pane_id"]
        .as_str()
        .or_else(|| result["root_pane"]["pane_id"].as_str())
        .filter(|id| !id.is_empty());
    let next = next_pane(request.action, &pane, created);
    let location = next.map_or_else(
        || "/".to_owned(),
        |next| format!("/term/{}", render::urlencode(next)),
    );
    // `location` for the web app, `pane` for the native one.
    Ok(Json(json!({ "location": location, "pane": next })))
}

/// The pane the phone shows after `action` on `pane`: the one it created,
/// the same one, or none (home) when it is gone.
pub(crate) fn next_pane<'a>(
    action: Action,
    pane: &'a str,
    created: Option<&'a str>,
) -> Option<&'a str> {
    match (action, created) {
        (Action::SplitRight | Action::SplitDown | Action::NewTab, Some(created)) => Some(created),
        (Action::ClosePane | Action::CloseTab, _) => None,
        _ => Some(pane),
    }
}

/// The longest name the phone gives a chat or a tab, in characters.
pub(crate) const MAX_LABEL: usize = 200;

#[derive(Deserialize)]
pub(super) struct RenameRequest {
    label: String,
}

/// What the phone renames: a chat by its pane, whose label the chat's
/// title reads first, or a terminal by its tab, whose label its row shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenameTarget<'a> {
    Chat { pane: &'a str },
    Tab { tab: &'a str },
}

/// The Herdr method and parameters that give `target` the name `label`.
/// An empty name clears a chat's label, so its title is the agent's again;
/// a tab always has a name.
pub(crate) fn rename_call(
    target: RenameTarget<'_>,
    label: &str,
) -> Result<(ApiMethod, Value), ApiError> {
    let label = label.trim();
    if label.chars().count() > MAX_LABEL {
        return Err(ApiError::Invalid("that name is too long"));
    }
    match target {
        RenameTarget::Chat { pane } if label.is_empty() => {
            Ok((ApiMethod::PaneRename, json!({ "pane_id": pane })))
        }
        RenameTarget::Chat { pane } => Ok((
            ApiMethod::PaneRename,
            json!({ "pane_id": pane, "label": label }),
        )),
        RenameTarget::Tab { .. } if label.is_empty() => {
            Err(ApiError::Invalid("a tab needs a name"))
        }
        RenameTarget::Tab { tab } => Ok((
            ApiMethod::TabRename,
            json!({ "tab_id": tab, "label": label }),
        )),
    }
}

/// Renames the chat `pane` runs, or the terminal tab it is in.
pub(super) async fn rename(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(request): Json<RenameRequest>,
) -> Result<Json<Value>, ApiError> {
    let (method, params) = {
        let layout = state.hub.layout();
        let target = if layout.chat(&pane).is_some() {
            RenameTarget::Chat { pane: &pane }
        } else {
            let (_, terminal) = layout.terminal(&pane).ok_or(ApiError::NotFound)?;
            RenameTarget::Tab { tab: &terminal.tab }
        };
        rename_call(target, &request.label)?
    };
    state.hub.call(method, params).await?;
    Ok(Json(json!({})))
}

//! New workspaces and chats for the native app: what the form offers
//! (`GET /api/v2/new`), and creating either with an optional first
//! prompt, which is sent once the chat's program reports in. The phone may
//! name the mode and auto-accepting a chat starts with; those go first, so
//! the first prompt never runs in a mode the person did not pick.

use crate::web::{
    ApiError, Device, NEW_CHAT_WAIT, Shared, check_new_workspace, folders, parse_agent,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{Agent, Command, MAX_PROMPT_SIZE, OptionValue, Rejection};
use std::{
    future::Future,
    time::{Duration, Instant},
};

/// How often a session still starting is asked again for its mode.
const START_RETRY: Duration = Duration::from_millis(200);

/// What the new workspace and new chat forms offer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct NewOptions {
    /// Every agent, in Herdr's order.
    pub(crate) agents: Vec<AgentChoice>,
    /// Folders workspaces already use.
    pub(crate) recent: Vec<String>,
    /// Each project root with the folders in it.
    pub(crate) roots: Vec<Root>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AgentChoice {
    /// What a request names it by.
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Root {
    pub(crate) folders: Vec<String>,
    pub(crate) path: String,
}

impl NewOptions {
    pub(crate) fn new(recent: Vec<String>, roots: Vec<(String, Vec<String>)>) -> Self {
        Self {
            agents: Agent::ALL
                .iter()
                .map(|agent| AgentChoice {
                    id: agent.name().to_owned(),
                    name: agent.display_name().to_owned(),
                })
                .collect(),
            recent,
            roots: roots
                .into_iter()
                .map(|(path, folders)| Root { folders, path })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct NewWorkspace {
    agent: String,
    #[serde(default)]
    branch: String,
    cwd: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    start: Option<Start>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NewChat {
    agent: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    start: Option<Start>,
}

/// What a new chat starts with, as last picked on the phone.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub(crate) struct Start {
    #[serde(default)]
    pub(crate) auto_accept: bool,
    /// The mode's id, or the choice's when `option` names a setting.
    #[serde(default)]
    pub(crate) mode: Option<String>,
    /// The agent's mode setting, for an agent that has one rather than
    /// the older list of modes.
    #[serde(default)]
    pub(crate) option: Option<String>,
}

impl Start {
    /// The command that picks the mode, if one was named.
    fn mode_command(&self) -> Option<Command> {
        let id = self.mode.clone()?;
        Some(match self.option.clone() {
            Some(option) => Command::SetOption {
                id: option,
                value: OptionValue::Choice { id },
            },
            None => Command::SetMode { id },
        })
    }
}

/// A workspace the phone created.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CreatedWorkspace {
    /// The new chat's pane; `None` when it was not listed in time, and
    /// the phone finds it on home later.
    pub(crate) pane: Option<String>,
    /// Whether the first prompt was sent.
    pub(crate) prompted: bool,
    /// Whether the chat took the mode and auto-accepting it was asked to
    /// start with.
    pub(crate) started: bool,
    pub(crate) workspace: String,
}

/// A chat the phone started.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CreatedChat {
    pub(crate) pane: Option<String>,
    pub(crate) prompted: bool,
    pub(crate) started: bool,
}

/// `GET /api/v2/new`.
pub(crate) async fn new_options(
    State(state): State<Shared>,
    _device: Device,
) -> Result<Json<NewOptions>, ApiError> {
    let layout = state.hub.layout();
    let folders = tokio::task::spawn_blocking(move || folders(&layout))
        .await
        .map_err(|_| ApiError::Folder)?;
    Ok(Json(NewOptions::new(folders.recent, folders.roots)))
}

/// `POST /api/v2/workspaces`.
pub(crate) async fn create_workspace(
    State(state): State<Shared>,
    _device: Device,
    Json(request): Json<NewWorkspace>,
) -> Result<Json<CreatedWorkspace>, ApiError> {
    check_prompt(&request.prompt)?;
    let (agent, branch) =
        check_new_workspace(&state, &request.cwd, &request.branch, &request.agent).await?;
    let created = state
        .hub
        .create_workspace(&request.cwd, branch.as_deref(), agent)
        .await?;
    let opened = open(&state, created.pane, request.start, request.prompt).await;
    Ok(Json(CreatedWorkspace {
        pane: opened.pane,
        prompted: opened.prompted,
        started: opened.started,
        workspace: created.workspace,
    }))
}

/// `POST /api/v2/workspaces/{workspace}/chats`.
pub(crate) async fn start_chat(
    State(state): State<Shared>,
    _device: Device,
    Path(workspace): Path<String>,
    Json(request): Json<NewChat>,
) -> Result<Json<CreatedChat>, ApiError> {
    check_prompt(&request.prompt)?;
    let agent = parse_agent(&request.agent)?;
    let pane = state.hub.start_chat(&workspace, agent, None).await?;
    let opened = open(&state, pane, request.start, request.prompt).await;
    Ok(Json(CreatedChat {
        pane: opened.pane,
        prompted: opened.prompted,
        started: opened.started,
    }))
}

/// Refuses a first prompt the chat would refuse anyway, before anything
/// is created.
fn check_prompt(prompt: &str) -> Result<(), ApiError> {
    if prompt.len() > MAX_PROMPT_SIZE {
        return Err(ApiError::Invalid("the prompt is too long"));
    }
    Ok(())
}

/// How opening a new chat went.
struct Opened {
    /// The pane, if it was listed in time.
    pane: Option<String>,
    prompted: bool,
    started: bool,
}

/// Waits for the chat in `pane` to be listed, gives it `start`, then sends
/// `prompt` unless it is blank. A failure here is not an error: the
/// workspace or chat exists either way, and the phone opens it to try
/// again. A prompt is held back when the chat did not take `start`, so it
/// cannot run in a mode the person did not pick.
async fn open(state: &Shared, pane: String, start: Option<Start>, prompt: String) -> Opened {
    if !state.hub.chat_ready(&pane, NEW_CHAT_WAIT).await {
        return Opened {
            pane: None,
            prompted: false,
            started: false,
        };
    }
    let started = match start {
        Some(start) => {
            apply_start(&start, NEW_CHAT_WAIT, |command| {
                state.hub.command(&pane, command)
            })
            .await
        }
        None => true,
    };
    if !started || prompt.trim().is_empty() {
        return Opened {
            pane: Some(pane),
            prompted: false,
            started,
        };
    }
    let sent = state
        .hub
        .command(
            &pane,
            Command::Prompt {
                text: prompt,
                attachments: Vec::new(),
            },
        )
        .await;
    Opened {
        pane: Some(pane),
        prompted: matches!(sent, Ok(None)),
        started,
    }
}

/// Sends `start`'s commands through `send`, and says whether the chat took
/// them all. A session refuses its mode until it is up, so that one is
/// asked again for up to `wait`.
pub(crate) async fn apply_start<F, E>(
    start: &Start,
    wait: Duration,
    mut send: impl FnMut(Command) -> F,
) -> bool
where
    F: Future<Output = Result<Option<Rejection>, E>>,
{
    let deadline = Instant::now() + wait;
    let mode = match start.mode_command() {
        None => true,
        Some(command) => loop {
            match send(command.clone()).await {
                Ok(None) => break true,
                Ok(Some(Rejection::NotReady)) if Instant::now() < deadline => {
                    tokio::time::sleep(START_RETRY).await;
                }
                Ok(Some(_)) | Err(_) => break false,
            }
        },
    };
    if !start.auto_accept {
        return mode;
    }
    let on = send(Command::SetAutoAccept { on: true }).await;
    mode && matches!(on, Ok(None))
}

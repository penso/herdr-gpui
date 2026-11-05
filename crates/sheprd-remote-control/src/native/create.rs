//! New workspaces and chats for the native app: what the form offers
//! (`GET /api/v2/new`), and creating either with an optional first
//! prompt, which is sent once the chat's program reports in.

use crate::web::{
    ApiError, Device, NEW_CHAT_WAIT, Shared, check_new_workspace, folders, parse_agent,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{Agent, Command, MAX_PROMPT_SIZE};
use std::time::Duration;

/// How long the form waits to learn which agents are installed. Finding
/// them only looks at a few folders, so this bounds a stuck disk.
const INSTALLED_WAIT: Duration = Duration::from_secs(2);

/// What the new workspace and new chat forms offer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct NewOptions {
    /// The agents installed on this computer, in Herdr's order.
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
    pub(crate) fn new(
        agents: &[Agent],
        recent: Vec<String>,
        roots: Vec<(String, Vec<String>)>,
    ) -> Self {
        Self {
            agents: agents
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
}

#[derive(Debug, Deserialize)]
pub(crate) struct NewChat {
    agent: String,
    #[serde(default)]
    prompt: String,
}

/// A workspace the phone created.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CreatedWorkspace {
    /// The new chat's pane; `None` when it was not listed in time, and
    /// the phone finds it on home later.
    pub(crate) pane: Option<String>,
    /// Whether the first prompt was sent.
    pub(crate) prompted: bool,
    pub(crate) workspace: String,
}

/// A chat the phone started.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CreatedChat {
    pub(crate) pane: Option<String>,
    pub(crate) prompted: bool,
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
    let agents = offered(installed_agents(state.hub.program()).await);
    Ok(Json(NewOptions::new(
        &agents,
        folders.recent,
        folders.roots,
    )))
}

/// What `sheprd-acp agents` lists, the same search that starts a chat, or
/// `None` when it could not say.
async fn installed_agents(program: Option<&std::path::Path>) -> Option<Vec<Agent>> {
    let program = program?.to_owned();
    let listing = tokio::task::spawn_blocking(move || list_agents(&program));
    tokio::time::timeout(INSTALLED_WAIT, listing)
        .await
        .ok()?
        .ok()?
}

pub(super) fn list_agents(program: &std::path::Path) -> Option<Vec<Agent>> {
    let output = sheprd_client::new_command(program)
        .arg("agents")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_agents(&String::from_utf8_lossy(&output.stdout)))
}

/// One agent name a line; a name this daemon does not know is skipped.
pub(super) fn parse_agents(listing: &str) -> Vec<Agent> {
    listing
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// The agents to offer: every one when none were found or the search
/// failed, as an agent installed outside the searched folders can still
/// start, and an empty form could start nothing.
pub(super) fn offered(installed: Option<Vec<Agent>>) -> Vec<Agent> {
    installed
        .filter(|agents| !agents.is_empty())
        .unwrap_or_else(|| Agent::ALL.to_vec())
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
    let (pane, prompted) = open(&state, created.pane, request.prompt).await;
    Ok(Json(CreatedWorkspace {
        pane,
        prompted,
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
    let (pane, prompted) = open(&state, pane, request.prompt).await;
    Ok(Json(CreatedChat { pane, prompted }))
}

/// Refuses a first prompt the chat would refuse anyway, before anything
/// is created.
fn check_prompt(prompt: &str) -> Result<(), ApiError> {
    if prompt.len() > MAX_PROMPT_SIZE {
        return Err(ApiError::Invalid("the prompt is too long"));
    }
    Ok(())
}

/// Waits for the chat in `pane` to be listed, then sends `prompt` unless
/// it is blank: the pane if it was listed in time, and whether the
/// prompt went. A prompt that fails is not an error: the workspace or
/// chat exists either way, and the phone opens it to try again.
async fn open(state: &Shared, pane: String, prompt: String) -> (Option<String>, bool) {
    if !state.hub.chat_ready(&pane, NEW_CHAT_WAIT).await {
        return (None, false);
    }
    if prompt.trim().is_empty() {
        return (Some(pane), false);
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
    (Some(pane), matches!(sent, Ok(None)))
}

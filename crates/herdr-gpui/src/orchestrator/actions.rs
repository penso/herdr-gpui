//! What the user asks the orchestrator to do: dispatch an agent, stop it,
//! send it text, remove its worktree, or delete a bead. Each runs on its own
//! short-lived thread with its own database connection, so a slow agent
//! start never holds up syncing, and writes only runs herdr-gpui owns.

use super::{
    Error, HerdrSession, Owner, Result, Run, RunState, SourceKey, Store, beads,
    error::script,
    model::{Backend, Workspace},
};
use crate::teleport::{AgentKind, Envelope, Host, WorktreeCreated};
use chrono::Utc;
use herdr_client::{ConnectTarget, shell_quote};
use std::{path::PathBuf, sync::atomic::AtomicBool};

/// How long `herdr agent start` may wait for the agent to be ready.
const AGENT_START_TIMEOUT_MS: &str = "90000";

/// Where actions run: the repository's host and the shared database.
#[derive(Clone)]
pub(crate) struct Site {
    pub(crate) host: Host,
    pub(crate) target: ConnectTarget,
    pub(crate) main_root: String,
    pub(crate) database: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DispatchRequest {
    /// The canonical key of the item the agent works on.
    pub(crate) item_key: String,
    pub(crate) kind: AgentKind,
    pub(crate) prompt: String,
    pub(crate) branch: String,
    /// The Herdr workspace of the repository, whose worktree list gets the
    /// new checkout.
    pub(crate) workspace_id: String,
    /// What the branch starts from; the main checkout's `HEAD` when `None`.
    pub(crate) base: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Dispatch(DispatchRequest),
    /// Interrupt a run's agent.
    Stop {
        run: String,
    },
    /// Type `text` to a run's agent as a prompt.
    Send {
        run: String,
        text: String,
    },
    /// Remove a run's worktree and forget the run.
    Remove {
        run: String,
    },
    /// Delete a bead for good.
    DeleteBead {
        source: SourceKey,
        id: String,
    },
}

impl Action {
    /// What the user reads once it succeeded.
    fn done(&self) -> &'static str {
        match self {
            Self::Dispatch(_) => "Agent started",
            Self::Stop { .. } => "Agent stopped",
            Self::Send { .. } => "Message sent",
            Self::Remove { .. } => "Worktree removed",
            Self::DeleteBead { .. } => "Bead deleted",
        }
    }
}

/// Runs `action` to completion; the success text, or why it failed.
pub(crate) fn perform(
    site: &Site,
    action: &Action,
    cancelled: &AtomicBool,
) -> Result<&'static str> {
    match action {
        Action::Dispatch(request) => dispatch(site, request, cancelled)?,
        Action::Stop { run } => {
            let (mut store, mut found, session) = own_run(site, run)?;
            for key in ["esc", "ctrl+c"] {
                herdr(
                    site,
                    &["agent", "send-keys", &session.pane_id, key],
                    "Stopping the agent",
                    cancelled,
                )?;
            }
            found.state = RunState::Cancelled;
            found.updated_at = Utc::now();
            store.save_run(&found, Some(&session))?;
        }
        Action::Send { run, text } => {
            let (_, _, session) = own_run(site, run)?;
            herdr(
                site,
                &["agent", "prompt", &session.pane_id, text],
                "Sending the message",
                cancelled,
            )?;
        }
        Action::Remove { run } => {
            let (mut store, found, _) = own_run(site, run)?;
            if let Some(workspace) = &found.workspace {
                herdr(
                    site,
                    &[
                        "worktree",
                        "remove",
                        "--workspace",
                        &workspace.id,
                        "--force",
                    ],
                    "Removing the worktree",
                    cancelled,
                )?;
            }
            store.delete_run(&found.id)?;
        }
        Action::DeleteBead { source, id } => beads::delete(&site.host, source, id, cancelled)?,
    }
    Ok(action.done())
}

/// A run herdr-gpui owns, with its session, read fresh from the database.
fn own_run(site: &Site, id: &str) -> Result<(Store, Run, HerdrSession)> {
    let store = Store::open(&site.database)?;
    let run = store
        .runs()?
        .into_iter()
        .find(|run| run.id == id)
        .ok_or_else(|| Error::RunNotFound(id.to_owned()))?;
    if run.owner != Owner::HerdrGpui {
        return Err(Error::NotOwner(id.to_owned()));
    }
    let session = store
        .sessions()?
        .into_iter()
        .find(|session| session.run_id == id)
        .ok_or_else(|| Error::RunNotFound(id.to_owned()))?;
    Ok((store, run, session))
}

fn herdr(
    site: &Site,
    args: &[&str],
    operation: &'static str,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    site.host
        .capture(&Host::herdr_line(args), cancelled)
        .map_err(script(operation))
}

/// The SSH destination and Herdr session a target names, as sessions record.
fn place(target: &ConnectTarget) -> (Option<String>, Option<String>) {
    match target {
        ConnectTarget::Ssh { target, session } => (Some(target.clone()), Some(session.clone())),
        ConnectTarget::Session { name, .. } => (None, Some(name.clone())),
        _ => (None, None),
    }
}

/// A Herdr agent name no other application or run uses.
fn agent_name() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("herdr-gpui-{}", &hex[..23])
}

fn dispatch(site: &Site, request: &DispatchRequest, cancelled: &AtomicBool) -> Result<()> {
    let mut store = Store::open(&site.database)?;
    let name = agent_name();
    let now = Utc::now();
    let mut run = Run {
        id: uuid::Uuid::new_v4().hyphenated().to_string(),
        item_key: request.item_key.clone(),
        workspace: None,
        agent: request.kind.name().to_owned(),
        model: None,
        state: RunState::Provisioning,
        message: None,
        session_id: Some(name.clone()),
        started_at: now,
        updated_at: now,
        owner: Owner::HerdrGpui,
    };
    store.save_run(&run, None)?;
    let mut created_workspace = None;
    let outcome = launch(
        site,
        request,
        &name,
        &mut run,
        &mut store,
        &mut created_workspace,
        cancelled,
    );
    if let Err(error) = &outcome {
        // Undo the checkout of a run that never got going; the branch stays.
        if let Some(workspace) = created_workspace {
            let _ = herdr(
                site,
                &["worktree", "remove", "--workspace", &workspace, "--force"],
                "Removing the worktree",
                &AtomicBool::new(false),
            );
        }
        run.state = RunState::Failed;
        run.message = Some(error.to_string().chars().take(500).collect());
        run.updated_at = Utc::now();
        store.save_run(&run, None)?;
    }
    outcome
}

fn launch(
    site: &Site,
    request: &DispatchRequest,
    name: &str,
    run: &mut Run,
    store: &mut Store,
    created_workspace: &mut Option<String>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let base = request.base.as_deref().unwrap_or("HEAD");
    let body = format!(
        "git -C {} rev-parse --verify --quiet {}\n",
        shell_quote(&site.main_root),
        shell_quote(&format!("{base}^{{commit}}")),
    );
    let commit = site
        .host
        .capture(&body, cancelled)
        .map_err(script("Reading the base commit"))?;
    let commit = String::from_utf8_lossy(&commit).trim().to_owned();
    if commit.len() < 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::Output {
            operation: "Reading the base commit",
        });
    }
    let output = herdr(
        site,
        &[
            "worktree",
            "create",
            "--workspace",
            &request.workspace_id,
            "--branch",
            &request.branch,
            "--base",
            &commit,
            "--no-focus",
        ],
        "Creating the worktree",
        cancelled,
    )?;
    let created = serde_json::from_slice::<Envelope<WorktreeCreated>>(&output)
        .map_err(|source| Error::OutputJson {
            operation: "Creating the worktree",
            source,
        })?
        .result;
    *created_workspace = Some(created.workspace.workspace_id.clone());
    let (host, session) = place(&site.target);
    run.workspace = Some(Workspace {
        backend: Backend::Herdr,
        id: created.workspace.workspace_id.clone(),
        host: host.clone(),
        path: Some(created.worktree.path.clone().into()),
        branch: request.branch.clone(),
    });
    run.state = RunState::Starting;
    run.updated_at = Utc::now();
    let herdr_session = HerdrSession {
        run_id: run.id.clone(),
        host,
        session,
        workspace_id: created.workspace.workspace_id,
        pane_id: created.root_pane.pane_id,
        agent_name: name.to_owned(),
        updated_at: run.updated_at,
    };
    store.save_run(run, Some(&herdr_session))?;
    herdr(
        site,
        &[
            "agent",
            "start",
            name,
            "--kind",
            request.kind.name(),
            "--pane",
            &herdr_session.pane_id,
            "--timeout",
            AGENT_START_TIMEOUT_MS,
        ],
        "Starting the agent",
        cancelled,
    )?;
    // The pane is the target: a name could match an agent started elsewhere.
    herdr(
        site,
        &["agent", "prompt", &herdr_session.pane_id, &request.prompt],
        "Sending the prompt",
        cancelled,
    )?;
    run.state = RunState::Running;
    run.updated_at = Utc::now();
    store.save_run(run, Some(&herdr_session))?;
    Ok(())
}

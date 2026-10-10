//! What the user asks the orchestrator to do: dispatch an agent, stop it,
//! send it text, remove its worktree, or delete a bead. Each runs on its own
//! short-lived thread with its own database connection, so a slow agent
//! start never holds up syncing, and writes only runs herdr-gpui owns.

use super::{
    Error, HerdrSession, Owner, Result, Run, RunState, SourceKey, Store, beads,
    error::script,
    model::{Backend, Workspace},
};
use crate::teleport::{
    self, AgentKind, Envelope, FreshOrigin, Host, HostRepositories, WorktreeCreated,
};
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

/// Another connected host to start the agent on, set up as smart dispatch
/// and fan-out do: the repository found or cloned there, the base commit
/// shipped when it lacks it.
#[derive(Clone, Debug)]
pub(crate) struct Elsewhere {
    pub(crate) origin: FreshOrigin,
    pub(crate) destination: HostRepositories,
    pub(crate) target: ConnectTarget,
}

#[derive(Clone, Debug)]
pub(crate) struct DispatchRequest {
    /// The canonical key of the item the agent works on.
    pub(crate) item_key: String,
    pub(crate) kind: AgentKind,
    /// Passed to the agent as `--model`, when chosen.
    pub(crate) model: Option<String>,
    pub(crate) prompt: String,
    pub(crate) branch: String,
    /// The Herdr workspace of the repository, whose worktree list gets the
    /// new checkout.
    pub(crate) workspace_id: String,
    /// What the branch starts from; the main checkout's `HEAD` when `None`.
    pub(crate) base: Option<String>,
    /// Another host to start on; the repository's own when `None`.
    pub(crate) elsewhere: Option<Elsewhere>,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Dispatch(Box<DispatchRequest>),
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
            Self::Remove { .. } => "Run removed",
            Self::DeleteBead { .. } => "Bead deleted",
        }
    }
}

/// Whether `model` is a name an agent's `--model` could take: no spaces,
/// controls, or leading dash, so it cannot become another option.
pub(crate) fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 128
        && !model.starts_with('-')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
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
            let (mut store, mut found, session, host) = own_run(site, run)?;
            for key in ["esc", "ctrl+c"] {
                herdr(
                    &host,
                    &["agent", "send-keys", &session.pane_id, key],
                    "Stopping the agent",
                    cancelled,
                )?;
            }
            found.state = RunState::Cancelled;
            found.updated_at = Utc::now();
            store.update_run(&found, Some(&session))?;
        }
        Action::Send { run, text } => {
            let (_, _, session, host) = own_run(site, run)?;
            herdr(
                &host,
                &["agent", "prompt", &session.pane_id, text],
                "Sending the message",
                cancelled,
            )?;
        }
        Action::Remove { run } => {
            let (mut store, found, session) = owned(site, run)?;
            // A run with no session, such as one that failed before its
            // worktree was made, has nothing on a host to remove.
            if let (Some(workspace), Some(session)) = (&found.workspace, &session) {
                herdr(
                    &host_of(site, session)?,
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

/// A run herdr-gpui owns, with its session and the host its agent is on,
/// read fresh from the database.
fn own_run(site: &Site, id: &str) -> Result<(Store, Run, HerdrSession, Host)> {
    let (store, run, session) = owned(site, id)?;
    let session = session.ok_or_else(|| Error::RunNotFound(id.to_owned()))?;
    let host = host_of(site, &session)?;
    Ok((store, run, session, host))
}

/// A run herdr-gpui owns and its session, if it has one, read fresh.
fn owned(site: &Site, id: &str) -> Result<(Store, Run, Option<HerdrSession>)> {
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
        .find(|session| session.run_id == id);
    Ok((store, run, session))
}

/// The host a session's agent is on: the repository's own unless the run was
/// dispatched elsewhere.
fn host_of(site: &Site, session: &HerdrSession) -> Result<Host> {
    if (session.host.clone(), session.session.clone()) == place(&site.target) {
        return Ok(site.host.clone());
    }
    Host::new(&target_of(session)).map_err(|_| Error::UnsupportedHost)
}

/// The target a session's host and Herdr session name.
pub(super) fn target_of(session: &HerdrSession) -> ConnectTarget {
    match (&session.host, &session.session) {
        (Some(destination), session) => ConnectTarget::Ssh {
            target: destination.clone(),
            session: session.clone().unwrap_or_else(|| "default".into()),
        },
        (None, Some(name)) => ConnectTarget::Session {
            name: name.clone(),
            development: false,
        },
        (None, None) => ConnectTarget::Local,
    }
}

fn herdr(
    host: &Host,
    args: &[&str],
    operation: &'static str,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    host.capture(&Host::herdr_line(args), cancelled)
        .map_err(script(operation))
}

/// The SSH destination and Herdr session a target names, as sessions record.
pub(super) fn place(target: &ConnectTarget) -> (Option<String>, Option<String>) {
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

/// Where the new worktree goes: the host, its target, the workspace whose
/// repository gets it, and the base commit.
struct Destination {
    host: Host,
    target: ConnectTarget,
    workspace_id: String,
    commit: String,
    prepared: Option<teleport::Prepared>,
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
        model: request.model.clone(),
        state: RunState::Provisioning,
        message: None,
        session_id: Some(name.clone()),
        started_at: now,
        updated_at: now,
        owner: Owner::HerdrGpui,
    };
    store.save_run(&run, None)?;
    let mut created: Option<(Host, String)> = None;
    let outcome = destination(site, request, cancelled).and_then(|destination| {
        let outcome = launch(
            &destination,
            request,
            &name,
            &mut run,
            &mut store,
            &mut created,
            cancelled,
        );
        // The new branch keeps the shipped commit; its reference can go.
        if let Some(prepared) = &destination.prepared {
            prepared.finish(&AtomicBool::new(false));
        }
        outcome
    });
    if let Err(error) = &outcome {
        // Undo the checkout of a run that never got going; the branch stays.
        // A checkout that would not go keeps its session, which names the
        // host, so Remove can try again; one that went is no longer the run's.
        let mut kept = None;
        if let Some((host, workspace)) = created {
            let removed = herdr(
                &host,
                &["worktree", "remove", "--workspace", &workspace, "--force"],
                "Removing the worktree",
                &AtomicBool::new(false),
            );
            if removed.is_ok() {
                run.workspace = None;
            } else {
                kept = store
                    .sessions()?
                    .into_iter()
                    .find(|session| session.run_id == run.id);
            }
        }
        run.state = RunState::Failed;
        run.message = Some(error.to_string().chars().take(500).collect());
        run.updated_at = Utc::now();
        // A run removed meanwhile stays removed; the failure is still told.
        match store.update_run(&run, kept.as_ref()) {
            Ok(()) | Err(Error::RunNotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    outcome
}

fn destination(
    site: &Site,
    request: &DispatchRequest,
    cancelled: &AtomicBool,
) -> Result<Destination> {
    let base = request.base.as_deref().unwrap_or("HEAD");
    if let Some(elsewhere) = &request.elsewhere {
        let commit =
            teleport::base_commit(&elsewhere.origin, base, cancelled).map_err(Error::Teleport)?;
        let prepared = teleport::prepare(
            &elsewhere.origin,
            &elsewhere.destination,
            &commit,
            &mut |_| {},
            cancelled,
        )
        .map_err(Error::Teleport)?;
        return Ok(Destination {
            host: elsewhere.destination.place.host.clone(),
            target: elsewhere.target.clone(),
            workspace_id: prepared.repository.workspace_id.clone(),
            commit,
            prepared: Some(prepared),
        });
    }
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
    Ok(Destination {
        host: site.host.clone(),
        target: site.target.clone(),
        workspace_id: request.workspace_id.clone(),
        commit,
        prepared: None,
    })
}

fn launch(
    destination: &Destination,
    request: &DispatchRequest,
    name: &str,
    run: &mut Run,
    store: &mut Store,
    created_workspace: &mut Option<(Host, String)>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let host = &destination.host;
    let output = herdr(
        host,
        &[
            "worktree",
            "create",
            "--workspace",
            &destination.workspace_id,
            "--branch",
            &request.branch,
            "--base",
            &destination.commit,
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
    *created_workspace = Some((host.clone(), created.workspace.workspace_id.clone()));
    let (ssh, session) = place(&destination.target);
    run.workspace = Some(Workspace {
        backend: Backend::Herdr,
        id: created.workspace.workspace_id.clone(),
        host: ssh.clone(),
        path: Some(created.worktree.path.clone().into()),
        branch: request.branch.clone(),
    });
    run.state = RunState::Starting;
    run.updated_at = Utc::now();
    let herdr_session = HerdrSession {
        run_id: run.id.clone(),
        host: ssh,
        session,
        workspace_id: created.workspace.workspace_id,
        pane_id: created.root_pane.pane_id,
        agent_name: name.to_owned(),
        updated_at: run.updated_at,
    };
    store.update_run(run, Some(&herdr_session))?;
    let mut start = vec![
        "agent",
        "start",
        name,
        "--kind",
        request.kind.name(),
        "--pane",
        &herdr_session.pane_id,
        "--timeout",
        AGENT_START_TIMEOUT_MS,
    ];
    let model = request.model.as_deref().filter(|model| valid_model(model));
    if let Some(model) = model {
        start.extend(["--", "--model", model]);
    }
    herdr(host, &start, "Starting the agent", cancelled)?;
    // The pane is the target: a name could match an agent started elsewhere.
    herdr(
        host,
        &["agent", "prompt", &herdr_session.pane_id, &request.prompt],
        "Sending the prompt",
        cancelled,
    )?;
    run.state = RunState::Running;
    run.updated_at = Utc::now();
    store.update_run(run, Some(&herdr_session))?;
    Ok(())
}

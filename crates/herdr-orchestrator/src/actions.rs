//! What the user asks the orchestrator to do: dispatch an agent, stop it,
//! send it text, remove its worktree, or delete a bead. Each runs on its own
//! short-lived thread with its own database connection, so a slow agent
//! start never holds up syncing, and writes only runs herdr-gpui owns.

use super::{
    Error, HerdrSession, Owner, Result, Run, RunState, SourceKey, Store, beads,
    error::script,
    model::{Backend, Workspace},
};
use chrono::Utc;
use herdr_client::{ConnectTarget, shell_quote};
use herdr_remote::teleport::{
    self, AgentKind, Envelope, FreshOrigin, Host, HostRepositories, HostSnapshot, SnapshotResult,
    WorkspaceCreated, WorktreeCreated,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};

pub mod pull_head;
mod start;

pub use pull_head::PullHead;

/// How long `herdr agent start` may wait for the agent to be ready.
const AGENT_START_TIMEOUT_MS: &str = "90000";

/// Where actions run: the repository's host and the shared database.
#[derive(Clone)]
pub struct Site {
    pub host: Host,
    pub target: ConnectTarget,
    pub main_root: String,
    pub database: PathBuf,
}

/// Another connected host to start the agent on, set up as smart dispatch
/// and fan-out do: the repository found or cloned there, the base commit
/// shipped when it lacks it.
#[derive(Clone, Debug)]
pub struct Elsewhere {
    pub origin: FreshOrigin,
    pub destination: HostRepositories,
    pub target: ConnectTarget,
}

#[derive(Clone, Debug)]
pub struct DispatchRequest {
    /// The canonical key of the item the agent works on.
    pub item_key: String,
    pub kind: AgentKind,
    /// Passed to the agent as `--model`, when chosen.
    pub model: Option<String>,
    /// Passed to the agent after `--model`, each one argument as written:
    /// Herdr quotes them for the pane's shell, so none is split or expanded.
    pub args: Vec<String>,
    pub prompt: String,
    pub branch: String,
    /// The Herdr workspace of the repository, whose worktree list gets the
    /// new checkout.
    pub workspace_id: String,
    /// What the branch starts from; the main checkout's `HEAD` when `None`.
    pub base: Option<String>,
    /// A review's pull request, whose head the branch starts from instead:
    /// fetched when the repository lacks it, and required of the checkout.
    pub pull_request: Option<PullHead>,
    /// Another host to start on; the repository's own when `None`.
    pub elsewhere: Option<Elsewhere>,
}

#[derive(Clone, Debug)]
pub enum Action {
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
pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 128
        && !model.starts_with('-')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
}

/// The most arguments an agent is started with, and the longest one.
pub const MAX_ARGS: usize = 32;
pub const MAX_ARG_BYTES: usize = 1024;

/// Whether `args` can follow an agent's executable: a bounded list of
/// bounded arguments without control characters, which Herdr refuses.
/// Leading dashes are the point, so options are not filtered as in a model.
pub fn valid_args(args: &[impl AsRef<str>]) -> bool {
    args.len() <= MAX_ARGS
        && args.iter().all(|arg| {
            let arg = arg.as_ref();
            arg.len() <= MAX_ARG_BYTES && !arg.chars().any(char::is_control)
        })
}

/// Runs `action` to completion; the success text, or why it failed.
pub fn perform(site: &Site, action: &Action, cancelled: &AtomicBool) -> Result<&'static str> {
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
                remove_worktree(site, session, workspace, cancelled)?;
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
pub fn target_of(session: &HerdrSession) -> ConnectTarget {
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
    host.herdr_exec(args, Host::QUERY_IDLE, cancelled)
        .map_err(script(operation))
}

/// A run's worktree, removed on the host its session lives on. Herdr's
/// `worktree remove` addresses the workspace the worktree was made for, and
/// Herdr forgets a workspace whose last pane has exited, so the workspace id
/// recorded at dispatch goes stale once the run's pane is gone. A workspace
/// the snapshot still lists goes through Herdr, which also closes its panes;
/// a gone one has its recorded checkout removed with git, which only the
/// workspace could have asked Herdr to do.
fn remove_worktree(
    site: &Site,
    session: &HerdrSession,
    workspace: &Workspace,
    cancelled: &AtomicBool,
) -> Result<()> {
    const READING: &str = "Reading the workspaces";
    const REMOVING: &str = "Removing the worktree";
    let host = host_of(site, session)?;
    let alive = host_snapshot(&host, READING, cancelled)?;
    if alive.workspace(&workspace.id).is_some() {
        herdr(
            &host,
            &[
                "worktree",
                "remove",
                "--workspace",
                &workspace.id,
                "--force",
            ],
            REMOVING,
            cancelled,
        )?;
        return Ok(());
    }
    // Without the workspace only its checkout could remain; a run without
    // a recorded one has nothing on the host to remove.
    let Some(path) = workspace.path.as_deref() else {
        return Ok(());
    };
    if host.is_local() {
        if !path.is_dir() {
            return Ok(());
        }
        let path = path.to_string_lossy();
        let git = |args: &[&str]| {
            host.exec("git", args, Host::QUERY_IDLE, cancelled)
                .map_err(script(REMOVING))
        };
        let common = git(&[
            "-C",
            &path,
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ])?;
        let common = String::from_utf8_lossy(&common);
        let git_dir = format!("--git-dir={}", common.trim_end_matches(['\r', '\n']));
        git(&[&git_dir, "worktree", "remove", "--force", &path])?;
        return Ok(());
    }
    let body = format!(
        "path={path}\n\
         [ -d \"$path\" ] || exit 0\n\
         common=$(git -C \"$path\" rev-parse --git-common-dir 2>/dev/null) || exit 1\n\
         git --git-dir=\"$common\" worktree remove --force \"$path\"\n",
        path = shell_quote(&path.to_string_lossy()),
    );
    host.capture(&body, cancelled).map_err(script(REMOVING))?;
    Ok(())
}

/// The SSH destination and Herdr session a target names, as sessions record.
pub fn place(target: &ConnectTarget) -> (Option<String>, Option<String>) {
    match target {
        ConnectTarget::Ssh { target, session } => (Some(target.clone()), Some(session.clone())),
        ConnectTarget::Session { name, .. } => (None, Some(name.clone())),
        _ => (None, None),
    }
}

/// The longest name Herdr's `agent start` accepts. A name must also start
/// with a lowercase letter and hold only lowercase letters, digits, `-`, or `_`.
const MAX_AGENT_NAME_LEN: usize = 32;

/// A Herdr agent name no other application or run uses.
pub fn agent_name() -> String {
    const PREFIX: &str = "herdr-gpui-";
    // A UUID's simple form is 32 hex digits, so the slice stays in range.
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("{PREFIX}{}", &hex[..MAX_AGENT_NAME_LEN - PREFIX.len()])
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
    if !valid_args(&request.args) {
        return Err(Error::AgentArguments);
    }
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
    let base = match &request.pull_request {
        Some(head) => {
            head.fetch(site, cancelled)?;
            head.sha.as_str()
        }
        None => request.base.as_deref().unwrap_or("HEAD"),
    };
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
    let commit = if site.host.is_local() {
        super::git::commit(std::path::Path::new(&site.main_root), base)?
    } else {
        let output = site
            .host
            .exec(
                "git",
                &[
                    "-C",
                    &site.main_root,
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{base}^{{commit}}"),
                ],
                Host::QUERY_IDLE,
                cancelled,
            )
            .map_err(script("Reading the base commit"))?;
        String::from_utf8_lossy(&output).trim().to_owned()
    };
    if commit.len() < 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::Output {
            operation: "Reading the base commit",
        });
    }
    Ok(Destination {
        host: site.host.clone(),
        target: site.target.clone(),
        workspace_id: parent_workspace(site, &request.workspace_id, cancelled)?,
        commit,
        prepared: None,
    })
}

/// The host's workspaces, tabs, and panes, as `herdr api snapshot` reports
/// them.
fn host_snapshot(
    host: &Host,
    operation: &'static str,
    cancelled: &AtomicBool,
) -> Result<HostSnapshot> {
    let output = herdr(host, &["api", "snapshot"], operation, cancelled)?;
    serde_json::from_slice::<Envelope<SnapshotResult>>(&output)
        .map(|envelope| envelope.result.snapshot)
        .map_err(|source| Error::OutputJson { operation, source })
}

/// The workspace new worktrees of `workspace_id`'s repository are created
/// from. Herdr creates them only from a repository's parent workspace, so a
/// view opened in a linked worktree goes through its parent, opening one on
/// the main checkout when none is.
fn parent_workspace(site: &Site, workspace_id: &str, cancelled: &AtomicBool) -> Result<String> {
    const READING: &str = "Reading the workspaces";
    const OPENING: &str = "Opening the repository";
    let snapshot = host_snapshot(&site.host, READING, cancelled)?;
    if let Some(parent) = parent_of(&snapshot, workspace_id) {
        return Ok(parent.to_owned());
    }
    let output = herdr(
        &site.host,
        &[
            "workspace",
            "create",
            "--cwd",
            &site.main_root,
            "--no-focus",
        ],
        OPENING,
        cancelled,
    )?;
    Ok(
        serde_json::from_slice::<Envelope<WorkspaceCreated>>(&output)
            .map_err(|source| Error::OutputJson {
                operation: OPENING,
                source,
            })?
            .result
            .workspace
            .workspace_id,
    )
}

/// `workspace_id` unless it is a linked worktree, else an open parent
/// workspace of its repository; `None` when no parent is open. A workspace
/// the snapshot lacks is passed through for Herdr to report.
pub fn parent_of<'a>(snapshot: &'a HostSnapshot, workspace_id: &'a str) -> Option<&'a str> {
    let Some(linked) = snapshot
        .workspace(workspace_id)
        .and_then(|workspace| workspace.worktree.as_ref())
        .filter(|tree| tree.is_linked_worktree)
    else {
        return Some(workspace_id);
    };
    snapshot
        .workspaces
        .iter()
        .find(|workspace| {
            workspace
                .worktree
                .as_ref()
                .is_some_and(|tree| tree.repo_key == linked.repo_key && !tree.is_linked_worktree)
        })
        .map(|workspace| workspace.workspace_id.as_str())
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
    if let Some(head) = &request.pull_request {
        head.verify(host, &created.worktree.path, &request.branch, cancelled)?;
    }
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
    if model.is_some() || !request.args.is_empty() {
        start.push("--");
    }
    if let Some(model) = model {
        start.extend(["--model", model]);
    }
    start.extend(request.args.iter().map(String::as_str));
    start::agent(
        host,
        &start,
        &herdr_session.pane_id,
        created.root_pane.terminal_id.as_deref(),
        cancelled,
    )?;
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

//! Everything the server shows comes through here: the local Herdr daemon's
//! layout ([`daemon`], [`layout`]) and one shared connection per chat pane
//! ([`chat`]). Chats are only ever reached at the address their endpoint id
//! derives, in this user's private directory, never at a path read from
//! daemon data: the same trust boundary as Sheprd's chat views.

mod api;
mod attach;
mod chat;
mod core;
mod daemon;
mod layout;
mod recency;
mod sessions;

pub(crate) use api::ApiMethod;
pub(crate) use chat::{Chat, Event, Summary, View};
#[cfg(test)]
pub(crate) use layout::TerminalAgent;
pub(crate) use layout::{Chat as ChatRow, Layout, Terminal, TerminalPane, Workspace};

use serde_json::{Value, json};
use sheprd_chat_protocol::{Agent, Command, EndpointId, Rejection};
use sheprd_client::ConnectTarget;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError, atomic::AtomicBool},
    time::Duration,
};
use tokio::sync::{broadcast, watch};

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Steps of every chat queued for the push notifier.
const ALL_QUEUE: usize = 4096;

/// How often a listed chat is checked for its link, which [`Hub::follow`]
/// makes on the same layout change without signalling.
const LINK_CHECK: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("too many requests are waiting")]
    Busy,
    #[error("the chat program cannot be reached")]
    ChatUnavailable,
    #[error(transparent)]
    Core(#[from] core::Error),
    #[error("Herdr refused: {0}")]
    Daemon(String),
    #[error("Herdr is not running on this computer")]
    NotConnected,
    #[error("sheprd-acp was not found beside sheprd-remote-control or on PATH")]
    ProgramMissing,
    #[error(transparent)]
    Request(#[from] sheprd_client::Error),
    #[error("Herdr's answer did not have the expected shape")]
    Response,
    #[error("Herdr did not answer in time")]
    Timeout,
    #[error("no such chat")]
    UnknownChat,
    #[error("no such workspace")]
    UnknownWorkspace,
    #[error("chats cannot start inside WSL from here; open a terminal tab and start it there")]
    WslChat,
    // keep-sorted end
}

/// A workspace [`Hub::create_workspace`] opened, and the pane its chat
/// starts in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreatedWorkspace {
    pub(crate) pane: String,
    pub(crate) workspace: String,
}

pub(crate) struct Hub {
    /// Owns the worker that projects the layout; dropping it disconnects.
    _daemon: daemon::Daemon,
    all: broadcast::Sender<(String, Event)>,
    api: api::Api,
    /// Where chat sockets live; `None` resolves from the environment.
    chat_dir: Option<PathBuf>,
    chats: Mutex<HashMap<String, (EndpointId, Arc<Chat>)>>,
    core: Option<Arc<core::Source>>,
    layout: watch::Receiver<Arc<Layout>>,
    /// The primary session's own layout, to tell whether it answers.
    primary: watch::Receiver<Arc<Layout>>,
    program: Option<PathBuf>,
    sessions: Arc<sessions::Sessions>,
}

impl Hub {
    /// Connects to the local daemon at `socket` (discovered when `None`)
    /// and every other running Herdr 1 session of this user, and keeps chat
    /// connections, found in `chat_dir`, in step with their layout, and the
    /// distributions saved in `wsl_catalog`. Must run inside a Tokio runtime.
    pub(crate) fn start(
        socket: Option<PathBuf>,
        chat_dir: Option<PathBuf>,
        wsl_catalog: Option<PathBuf>,
        core_endpoint: crate::core_target::CoreTarget,
        state_dir: &Path,
    ) -> std::io::Result<Arc<Self>> {
        let (sender, layout) = watch::channel(Arc::new(Layout::default()));
        let api = api::Api::new(socket.clone());
        let primary_socket = socket
            .clone()
            .or_else(|| ConnectTarget::Local.socket_path().ok());
        let target = socket.map_or(ConnectTarget::Local, ConnectTarget::Socket);
        let (daemon_sender, daemon_layout) = watch::channel(Arc::new(Layout::default()));
        let daemon = daemon::Daemon::start(
            daemon_sender,
            target.clone(),
            primary_socket.as_deref().and_then(sessions::session_name),
        )?;
        let (core_sender, core_layout) = watch::channel(Arc::new(Layout::default()));
        let core = core::Source::start(core_sender, &target, core_endpoint, state_dir)?;
        let (base_sender, base_layout) = watch::channel(Arc::new(Layout::default()));
        tokio::spawn(core::merge_sources(
            base_sender,
            daemon_layout.clone(),
            core_layout,
        ));
        let sessions = sessions::Sessions::start(primary_socket, wsl_catalog);
        tokio::spawn(sessions::merge_sources(
            sender,
            base_layout,
            sessions.clone(),
        ));
        let (all, _) = broadcast::channel(ALL_QUEUE);
        let hub = Arc::new(Self {
            _daemon: daemon,
            all,
            api,
            chat_dir,
            chats: Mutex::new(HashMap::new()),
            core: Some(core),
            layout,
            primary: daemon_layout,
            program: locate_program(),
            sessions,
        });
        let follower = hub.clone();
        tokio::spawn(async move { follower.follow().await });
        Ok(hub)
    }

    /// The `sheprd-acp` chats start with, when one was found.
    pub(crate) fn program(&self) -> Option<&Path> {
        self.program.as_deref()
    }

    /// A hub showing `layout` that never connects to a daemon or a chat,
    /// for tests that must not reach a real Herdr.
    #[cfg(test)]
    pub(crate) fn offline(layout: Layout) -> Arc<Self> {
        Arc::new(Self::offline_in(layout, None, None))
    }

    /// An offline hub whose API calls go to the socket beside
    /// `client_socket`, for tests that answer them themselves.
    #[cfg(all(test, unix))]
    pub(crate) fn offline_with_api(layout: Layout, client_socket: PathBuf) -> Arc<Self> {
        Arc::new(Self::offline_in(layout, None, Some(client_socket)))
    }

    /// An offline hub that connects to the chats of `layout` in `chat_dir`,
    /// for tests that serve a chat program of their own.
    #[cfg(all(test, unix))]
    pub(crate) fn offline_with_chats(layout: Layout, chat_dir: PathBuf) -> Arc<Self> {
        let hub = Self::offline_in(layout, Some(chat_dir), None);
        hub.reconcile(&hub.layout());
        Arc::new(hub)
    }

    /// An offline hub whose primary does not answer, beside one session
    /// `name` listing `layout` whose API calls go to the socket beside
    /// `client_socket`.
    #[cfg(all(test, unix))]
    pub(crate) fn offline_with_session(
        layout: Layout,
        name: &str,
        client_socket: PathBuf,
    ) -> Arc<Self> {
        let merged = sessions::merge(
            &Layout::default(),
            &[(name.to_owned(), Arc::new(layout.clone()))],
        );
        let mut hub = Self::offline_in(merged, None, None);
        hub.primary = watch::channel(Arc::new(Layout::default())).1;
        hub.sessions = sessions::Sessions::offline_with(name, layout, client_socket);
        Arc::new(hub)
    }

    #[cfg(test)]
    fn offline_in(
        layout: Layout,
        chat_dir: Option<PathBuf>,
        client_socket: Option<PathBuf>,
    ) -> Self {
        let (sender, receiver) = watch::channel(Arc::new(layout));
        let (all, _) = broadcast::channel(ALL_QUEUE);
        Self {
            _daemon: daemon::Daemon::offline(sender),
            all,
            // No socket answers there unless a test serves one, so a request
            // fails rather than reach a real Herdr.
            api: api::Api::new(Some(
                client_socket.unwrap_or_else(|| PathBuf::from("/nonexistent/herdr-client.sock")),
            )),
            chat_dir,
            chats: Mutex::new(HashMap::new()),
            core: None,
            primary: receiver.clone(),
            layout: receiver,
            program: None,
            sessions: sessions::Sessions::offline(),
        }
    }

    pub(crate) fn layout(&self) -> Arc<Layout> {
        self.layout.borrow().clone()
    }

    pub(crate) fn watch_layout(&self) -> watch::Receiver<Arc<Layout>> {
        self.layout.clone()
    }

    /// Every step of every chat, tagged with its pane.
    pub(crate) fn subscribe_all(&self) -> broadcast::Receiver<(String, Event)> {
        self.all.subscribe()
    }

    pub(crate) fn chat(&self, pane: &str) -> Option<Arc<Chat>> {
        self.chats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(pane)
            .map(|(_, chat)| chat.clone())
    }

    pub(crate) async fn command(&self, pane: &str, command: Command) -> Result<Option<Rejection>> {
        let chat = self.chat(pane).ok_or(Error::UnknownChat)?;
        chat.command(command).await
    }

    /// Opens a tab in `workspace` running a chat with `agent`, in `cwd` or
    /// the workspace's folder, as Sheprd's `chat new` does. Returns the
    /// new chat's pane id.
    pub(crate) async fn start_chat(
        &self,
        workspace: &str,
        agent: Agent,
        cwd: Option<&str>,
    ) -> Result<String> {
        // The program here is this computer's: it cannot run in a
        // distribution, and its endpoint could not be reached there.
        if sessions::in_wsl(workspace) {
            return Err(Error::WslChat);
        }
        let program = self.program.as_deref().ok_or(Error::ProgramMissing)?;
        let layout = self.layout();
        let folder = layout
            .workspace(workspace)
            .ok_or(Error::UnknownWorkspace)?
            .cwd
            .clone();
        if let Some(core) = self.active_core() {
            let (cwd, program) = (cwd.unwrap_or(&folder).to_owned(), program.to_owned());
            return on_core(move || core.open_chat(agent, &program, &cwd)).await;
        }
        let response = self
            .call(
                ApiMethod::TabCreate,
                tab_params(workspace, cwd.unwrap_or(&folder), agent, program),
            )
            .await?;
        let (_, pane) = created_tab(&response).ok_or(Error::Response)?;
        self.run_line(&pane, &command_line(agent)).await?;
        Ok(pane)
    }

    /// Waits up to `limit` for the chat in `pane` to be listed and linked,
    /// so a phone can open it. Returns whether it got there.
    pub(crate) async fn chat_ready(&self, pane: &str, limit: Duration) -> bool {
        let mut layout = self.layout.clone();
        tokio::time::timeout(
            limit,
            chat_listed_and_linked(&mut layout, pane, || self.chat(pane).is_some()),
        )
        .await
        .is_ok()
    }

    /// A new workspace in `cwd`, on a new worktree when `branch` is given,
    /// with a chat running `agent` in it, in the Herdr 1 `session` this
    /// daemon follows by that name (the primary otherwise). While a core answers the chat
    /// starts there, in its unlisted group: the core has no workspaces or
    /// worktrees, so a branch is refused rather than sent to Herdr 1.
    pub(crate) async fn create_workspace(
        &self,
        cwd: &str,
        branch: Option<&str>,
        agent: Agent,
        session: Option<&str>,
    ) -> Result<CreatedWorkspace> {
        // As in [`Hub::start_chat`]: the program here cannot run in a
        // distribution, and a core would open the chat on this computer.
        if session.is_some_and(sessions::is_wsl) {
            return Err(Error::WslChat);
        }
        let program = self.program.as_deref().ok_or(Error::ProgramMissing)?;
        if let Some(core) = self.active_core() {
            if branch.is_some() {
                return Err(core::Error::Unsupported.into());
            }
            let (cwd, program) = (cwd.to_owned(), program.to_owned());
            let pane = on_core(move || core.open_chat(agent, &program, &cwd)).await?;
            return Ok(CreatedWorkspace {
                workspace: core::UNLISTED.to_owned(),
                pane,
            });
        }
        let opened = self
            .call_in(
                session,
                ApiMethod::WorkspaceCreate,
                json!({"cwd": cwd, "focus": false}),
            )
            .await?;
        let workspace = opened["workspace"]["workspace_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or(Error::Response)?
            .to_owned();
        let Some(branch) = branch else {
            let response = self
                .call(
                    ApiMethod::TabCreate,
                    tab_params(&workspace, cwd, agent, program),
                )
                .await?;
            let (_, pane) = created_tab(&response).ok_or(Error::Response)?;
            self.run_line(&pane, &command_line(agent)).await?;
            return Ok(CreatedWorkspace { workspace, pane });
        };
        let created = self
            .call(
                ApiMethod::WorktreeCreate,
                json!({"workspace_id": workspace, "branch": branch, "focus": false}),
            )
            .await?;
        let pane = created["root_pane"]["pane_id"]
            .as_str()
            .ok_or(Error::Response)?
            .to_owned();
        let worktree = created["workspace"]["workspace_id"]
            .as_str()
            .ok_or(Error::Response)?
            .to_owned();
        // The worktree's first pane is a shell in its folder; the program
        // path rides in this line because the pane was not made with it.
        self.run_line(&pane, &direct_command_line(program, agent))
            .await?;
        Ok(CreatedWorkspace {
            workspace: worktree,
            pane,
        })
    }

    /// Types `line` and Enter into `pane`, as Sheprd starts a chat.
    async fn run_line(&self, pane: &str, line: &str) -> Result<()> {
        self.call(
            ApiMethod::PaneSendInput,
            json!({"pane_id": pane, "text": line, "keys": ["Enter"]}),
        )
        .await?;
        Ok(())
    }

    /// The core new work goes to, while one answers.
    fn active_core(&self) -> Option<Arc<core::Source>> {
        self.core.clone().filter(|core| core.connected())
    }

    /// A new tab on the core for `workspace`'s folder, answered as Herdr
    /// answers one, once listed so a phone can open it. `None` when no
    /// core answers, or `workspace` is in WSL, and the tab is Herdr 1's to make.
    pub(crate) async fn new_core_tab(&self, workspace: &str) -> Result<Option<Value>> {
        // A core on this computer knows neither a distribution nor its paths.
        if sessions::in_wsl(workspace) {
            return Ok(None);
        }
        let Some(core) = self.active_core() else {
            return Ok(None);
        };
        let cwd = self
            .layout()
            .workspace(workspace)
            .map_or_else(String::new, |workspace| workspace.cwd.clone());
        let pane = on_core(move || core.open_shell(&cwd)).await?;
        let mut layout = self.layout.clone();
        let listed = layout.wait_for(|layout| layout.terminal(&pane).is_some());
        // An unlisted tab is still made: the phone reloads and finds it.
        let _ = tokio::time::timeout(CORE_LISTING, listed).await;
        Ok(Some(json!({"pane": {"pane_id": pane}})))
    }

    /// Calls Herdr's public API, for the terminal views.
    pub(crate) async fn call(&self, method: ApiMethod, params: Value) -> Result<Value> {
        if core::owns(&params) {
            if !self.layout().connected {
                return Err(Error::NotConnected);
            }
            let core = self.core.clone().ok_or(core::Error::Disconnected)?;
            return tokio::task::spawn_blocking(move || core.call(method, &params))
                .await
                .map_err(|_| core::Error::Disconnected)?;
        }
        if let Some((session, params)) = self.session_for(method, &params)? {
            let mut answer = session.api.call(method, params).await?;
            sessions::prefix_answer(&mut answer, &session.name);
            return Ok(answer);
        }
        match self.api.call(method, params).await {
            Err(Error::NotConnected) if self.core.as_ref().is_some_and(|core| core.connected()) => {
                Err(core::Error::Unsupported.into())
            }
            result => result,
        }
    }

    /// [`Hub::call`] for a request whose ids do not say where it goes, such
    /// as a new workspace: to the session this daemon follows by `session`'s
    /// name, else as `call` sends it. The primary's own name is not among
    /// those followed, so it goes to the primary.
    pub(crate) async fn call_in(
        &self,
        session: Option<&str>,
        method: ApiMethod,
        params: Value,
    ) -> Result<Value> {
        let Some(session) = session.and_then(|name| self.sessions.get(name)) else {
            return self.call(method, params).await;
        };
        let mut answer = session.api.call(method, params).await?;
        sessions::prefix_answer(&mut answer, &session.name);
        Ok(answer)
    }

    /// Where `path`, a file on this computer, is for the program in `pane`
    /// when that pane is in a WSL distribution; `None` when it is not, and
    /// the path reads the same there.
    pub(crate) async fn wsl_path(&self, pane: &str, path: &Path) -> Result<Option<String>> {
        if !sessions::in_wsl(pane) {
            return Ok(None);
        }
        let (name, _) = sessions::owner(&json!({"pane_id": pane}))?.ok_or(Error::Response)?;
        let session = self.sessions.get(&name).ok_or(Error::NotConnected)?;
        let ConnectTarget::Wsl { distro, .. } = session.api.target() else {
            return Ok(None);
        };
        let path = path.to_owned();
        let mut paths = tokio::task::spawn_blocking(move || {
            sheprd_client::wsl_paths(&distro, &[path], &AtomicBool::new(false))
        })
        .await
        .map_err(|_| Error::NotConnected)??;
        paths.pop().map(Some).ok_or(Error::Response)
    }

    /// The named session a request is for, with its ids as that session
    /// knows them; `None` for the primary.
    fn session_for(
        &self,
        method: ApiMethod,
        params: &Value,
    ) -> Result<Option<(Arc<sessions::Session>, Value)>> {
        if let Some((name, params)) = sessions::owner(params)? {
            let session = self.sessions.get(&name).ok_or(Error::NotConnected)?;
            return Ok(Some((session, params)));
        }
        // A new workspace has no id to route by: it goes to the primary, or
        // to a session when only sessions answer.
        if method == ApiMethod::WorkspaceCreate && !self.primary.borrow().connected {
            return Ok(self
                .sessions
                .first_connected()
                .map(|session| (session, params.clone())));
        }
        Ok(None)
    }

    /// Starts connections for new chat panes and closes those whose pane
    /// went away or whose program changed.
    async fn follow(self: Arc<Self>) {
        let mut layout = self.layout.clone();
        loop {
            let current = layout.borrow_and_update().clone();
            self.reconcile(&current);
            if layout.changed().await.is_err() {
                return;
            }
        }
    }

    fn reconcile(&self, layout: &Layout) {
        let mut chats = self.chats.lock().unwrap_or_else(PoisonError::into_inner);
        chats.retain(|pane, (endpoint, chat)| {
            let keep = layout
                .chat(pane)
                .is_some_and(|(_, current)| &current.endpoint == endpoint);
            if !keep {
                chat.close();
            }
            keep
        });
        for chat in layout.chats() {
            if chats.contains_key(&chat.pane) {
                continue;
            }
            let address = match (&self.chat_dir, cfg!(windows)) {
                // Windows chats listen on named pipes, not in a folder.
                (Some(dir), false) => {
                    Some(sheprd_chat_protocol::endpoint_path(dir, &chat.endpoint))
                }
                _ => sheprd_chat_protocol::endpoint_address(&chat.endpoint),
            };
            let Some(address) = address else {
                continue;
            };
            if let Ok(link) = Chat::start(chat.pane.clone(), address, self.all.clone()) {
                chats.insert(chat.pane.clone(), (chat.endpoint.clone(), link));
            }
        }
    }
}

/// The environment variable the typed line runs the program from, as in
/// Sheprd.
const PROGRAM_ENV: &str = "HERDR_ACP";
/// How long a new core tab waits to be listed: a few of the core's polls.
const CORE_LISTING: Duration = Duration::from_secs(2);
const PROGRAM: &str = if cfg!(windows) {
    "herdr-acp.exe"
} else {
    "herdr-acp"
};

/// The `herdr-acp` beside this executable (the app bundle ships both),
/// else the first on `PATH`.
fn locate_program() -> Option<PathBuf> {
    let renamed = PROGRAM.replacen("herdr-", "sheprd-", 1);
    locate_named_program(&renamed).or_else(|| locate_named_program(PROGRAM))
}

fn locate_named_program(program: &str) -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(program)));
    beside
        .into_iter()
        .chain(
            std::env::var_os("PATH")
                .into_iter()
                .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(program)),
        )
        .find(|candidate| candidate.is_file())
}

/// Runs blocking core work off the runtime.
async fn on_core<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::from(core::Error::Disconnected))?
}

fn tab_params(workspace: &str, cwd: &str, agent: Agent, program: &Path) -> Value {
    json!({
        "workspace_id": workspace,
        "label": agent.display_name(),
        "focus": false,
        "cwd": cwd,
        "env": { PROGRAM_ENV: program.to_string_lossy() },
    })
}

/// The tab and root pane a `tab_created` answer names.
fn created_tab(result: &Value) -> Option<(String, String)> {
    let id = |value: &Value| {
        value
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    };
    (result["type"] == "tab_created").then_some(())?;
    Some((
        id(&result["tab"]["tab_id"])?,
        id(&result["root_pane"]["pane_id"])?,
    ))
}

/// Returns once `layout` lists the chat in `pane` and `linked` says its
/// connection is up; the caller bounds the wait.
async fn chat_listed_and_linked(
    layout: &mut watch::Receiver<Arc<Layout>>,
    pane: &str,
    linked: impl Fn() -> bool,
) {
    loop {
        let listed = layout.borrow_and_update().chat(pane).is_some();
        if listed && linked() {
            return;
        }
        if listed {
            tokio::time::sleep(LINK_CHECK).await;
        } else if layout.changed().await.is_err() {
            // The daemon watcher is gone, so the chat never appears.
            std::future::pending::<()>().await;
        }
    }
}

/// The line typed into a new chat tab, the same as Sheprd types: the
/// program comes from the tab's environment, so nothing typed depends on
/// where it is installed.
fn command_line(agent: Agent) -> String {
    if cfg!(windows) {
        format!("& $env:{PROGRAM_ENV} {agent}")
    } else {
        format!(" sh -c 'exec \"${PROGRAM_ENV}\" {agent}'")
    }
}

/// The line for a pane made without the program in its environment.
fn direct_command_line(program: &Path, agent: Agent) -> String {
    let program = program.to_string_lossy();
    if cfg!(windows) {
        format!("& '{}' {agent}", program.replace('\'', "''"))
    } else {
        format!(" {} {agent}", sheprd_client::shell_quote(&program))
    }
}

/// A terminal row, for other modules' tests.
#[cfg(test)]
pub(crate) fn layout_terminal(tab: &str, label: &str) -> Terminal {
    Terminal {
        tab: tab.into(),
        label: label.into(),
        cwd: None,
        panes: vec![TerminalPane {
            pane: format!("{tab}-p1"),
            label: None,
            cwd: None,
            focused: true,
        }],
        zoomed: false,
        agent: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

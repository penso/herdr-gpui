//! Starting a new chat from this window: a tab in the focused workspace, in
//! the focused pane's folder, running the `herdr-acp` that came with this
//! app for the agent the user picked. Once the program reports its pane,
//! discovery attaches the chat view as for any other chat.
//!
//! The endpoint API has no method that starts a command, so this follows the
//! closed-agent reopen: `tab.create` through the connection's script slot,
//! then one fixed line typed into the shell of the pane it made. The
//! program's path rides in the tab's environment rather than in the line,
//! so nothing typed depends on where the app is installed, and the agent's
//! name comes from the shared table, never from daemon text.

use crate::{HerdrWindow, NavigationTarget, window::Flash};
use gpui::{prelude::*, *};
use herdr_chat_protocol::Agent;
use herdr_client::Method;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The environment variable the typed line runs the program from.
pub(crate) const PROGRAM_ENV: &str = "HERDR_ACP";
/// The program's file name on this platform.
const PROGRAM: &str = if cfg!(windows) {
    "herdr-acp.exe"
} else {
    "herdr-acp"
};
/// How long a start may wait for the daemon to make its tab.
const DEADLINE: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{} is not installed on this host", .0.display_name())]
    AgentMissing(Agent),
    #[error("ACP chats are off in Settings")]
    Disabled,
    #[error("Herdr UI is not connected")]
    NotConnected,
    #[error("chats start on this machine's Herdr or an SSH host's")]
    NotLocal,
    #[error("no workspace is focused")]
    NoWorkspace,
    #[error("herdr-acp was not found beside Herdr UI or on PATH")]
    ProgramMissing,
    #[error("Herdr UI is still looking for herdr-acp")]
    ProgramUnknown,
    #[error("{0}")]
    Request(#[source] std::sync::Arc<crate::Error>),
    #[error("Herdr's answer did not name the chat's tab")]
    Response,
    #[error("another tab is still opening")]
    SlotBusy,
    #[error("Herdr did not open the chat's tab in time")]
    Timeout,
    // keep-sorted end
}

/// Where the `herdr-acp` this window starts chats with is, once looked for.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Program {
    #[default]
    Unknown,
    Looking,
    Found(PathBuf),
    Missing,
}

/// The program beside this app's executable, as the bundle and `cargo
/// build` lay it out, else the first on the login shell's `PATH`. Blocking:
/// run it off the UI thread.
pub(crate) fn locate() -> Option<PathBuf> {
    crate::bundled::locate(PROGRAM)
}

#[cfg(test)]
pub(crate) fn locate_in(exe: Option<&Path>, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    crate::bundled::locate_in(PROGRAM, exe, path)
}

/// The line typed into the new tab's shell. On Unix `sh` runs it, so it
/// reads the same from bash, zsh, fish, or nushell; on Windows it is the
/// PowerShell Herdr gives panes. A leading space keeps it out of the history
/// of shells that honor that. The shell stays when the chat quits, so an
/// agent that failed to start leaves its message on screen.
pub(crate) fn command_line(agent: Agent) -> String {
    if cfg!(windows) {
        format!("& $env:{PROGRAM_ENV} {agent}")
    } else {
        format!(" sh -c 'exec \"${PROGRAM_ENV}\" {agent}'")
    }
}

/// The line typed into a new tab on an SSH host: the first `herdr-acp`
/// found there, `~/.herdr/bin` before the rest, else a message saying where
/// to install it, which stays on screen.
pub(crate) fn remote_command_line(agent: Agent) -> String {
    format!(
        " sh -c '{}'",
        super::remote_program_script(&agent.to_string())
    )
}

/// The `tab.create` a chat runs in, labelled after its agent, with the
/// `[chat.env]` variables in its environment. A local chat also carries
/// this app's program there; a remote one finds its host's own.
pub(crate) fn tab_params(
    workspace: &str,
    cwd: Option<&str>,
    agent: Agent,
    program: Option<&Path>,
    extra: &std::collections::BTreeMap<String, String>,
) -> serde_json::Value {
    let mut params = json!({
        "workspace_id": workspace,
        "label": agent.display_name(),
        "focus": true,
    });
    let mut env: serde_json::Map<String, serde_json::Value> = extra
        .iter()
        .map(|(name, value)| (name.clone(), json!(value)))
        .collect();
    if let Some(program) = program {
        env.insert(PROGRAM_ENV.to_owned(), json!(program.to_string_lossy()));
    }
    if !env.is_empty() {
        params["env"] = serde_json::Value::Object(env);
    }
    if let Some(cwd) = cwd {
        params["cwd"] = json!(cwd);
    }
    params
}

/// A chat whose tab is being made, fenced by the selection, connection, and
/// daemon boot it was asked of, like a worktree script.
#[derive(Debug)]
pub(crate) struct Starting {
    agent: Agent,
    /// Whether the chat runs on an SSH host, so its line finds the host's
    /// own program.
    remote: bool,
    boot: String,
    deadline: Instant,
    endpoint: (u64, u64),
    endpoint_id: String,
    request: String,
}

impl HerdrWindow {
    /// Looks for the program once, off the UI thread.
    pub(super) fn locate_chat_program(&mut self, cx: &mut Context<Self>) {
        if self.chats.program != Program::Unknown {
            return;
        }
        self.chats.program = Program::Looking;
        let found = cx.background_executor().spawn(async { locate() });
        cx.spawn(async move |this, cx| {
            let found = found.await;
            let _ = this.update(cx, |this, cx| {
                this.chats.program = found.map_or(Program::Missing, Program::Found);
                cx.notify();
            });
        })
        .detach();
    }

    /// Whether this window can start a chat now: the selected Herdr runs on
    /// this machine and the program is here, or it runs on an SSH host,
    /// whose own program the new tab looks for.
    pub(crate) fn can_start_chat(&self) -> bool {
        if !self.config.features.acp_chats {
            return false;
        }
        match self.chat_route() {
            Some(super::Route::Local) => matches!(self.chats.program, Program::Found(_)),
            Some(super::Route::Ssh(_)) => true,
            None => false,
        }
    }

    fn chat_route(&self) -> Option<super::Route> {
        self.endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| super::discovery::route(&endpoint.connection.target, &self.live))
    }

    pub(crate) fn chat_starting(&self) -> bool {
        self.chats.starting.is_some()
    }

    /// Opens a tab running a chat with `agent` in the workspace of `beside`,
    /// else of the focused pane, in that pane's folder.
    pub(crate) fn start_chat(&mut self, agent: Agent, beside: Option<&str>) -> Result<(), Error> {
        if !self.config.features.acp_chats {
            return Err(Error::Disabled);
        }
        if self.chat_agent_missing(agent) {
            return Err(Error::AgentMissing(agent));
        }
        let program = match self.chat_route().ok_or(Error::NotLocal)? {
            super::Route::Ssh(_) => None,
            super::Route::Local => match &self.chats.program {
                Program::Found(program) => Some(program.clone()),
                Program::Missing => return Err(Error::ProgramMissing),
                Program::Unknown | Program::Looking => return Err(Error::ProgramUnknown),
            },
        };
        if self.script_slot_busy() {
            return Err(Error::SlotBusy);
        }
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .ok_or(Error::NotConnected)?;
        let pane = beside
            .or(snapshot.focused_pane_id.as_deref())
            .and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id));
        let workspace = pane
            .map(|pane| pane.workspace_id.clone())
            .or_else(|| snapshot.focused_workspace_id.clone())
            .ok_or(Error::NoWorkspace)?;
        let cwd = pane.and_then(|pane| pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()));
        let boot = snapshot.boot_id.clone();
        let endpoint = &self.endpoints[self.selected_endpoint];
        let request = endpoint
            .connection
            .request_script(
                &boot,
                Method::TabCreate,
                tab_params(
                    &workspace,
                    cwd.as_deref(),
                    agent,
                    program.as_deref(),
                    &self.config.chat.env,
                ),
            )
            .map_err(|_| Error::NotConnected)?;
        self.chats.starting = Some(Starting {
            agent,
            remote: program.is_none(),
            boot,
            deadline: Instant::now() + DEADLINE,
            endpoint: (self.selection_epoch, endpoint.generation),
            endpoint_id: endpoint.id.clone(),
            request,
        });
        Ok(())
    }

    /// [`Self::start_chat`], reporting a failure as a flash.
    pub(crate) fn start_chat_or_flash(&mut self, agent: Agent, cx: &mut Context<Self>) {
        if let Err(error) = self.start_chat(agent, None) {
            self.show_flash(
                Flash::warning(format!("The chat did not start: {error}")),
                cx,
            );
        }
        cx.notify();
    }

    /// Runs every tick: once the tab exists, types the line that starts the
    /// chat and shows the tab.
    pub(super) fn poll_chat_start(&mut self, cx: &mut Context<Self>) {
        let Some(starting) = &self.chats.starting else {
            return;
        };
        let current = starting.endpoint
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
            && self.live.status.is_connected()
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == starting.boot);
        if !current {
            self.chats.starting = None;
            return self.show_flash(
                Flash::warning(format!(
                    "The chat did not start: {}",
                    crate::Error::StaleConnection
                )),
                cx,
            );
        }
        if Instant::now() >= starting.deadline {
            self.chats.starting = None;
            return self.show_flash(
                Flash::warning(format!("The chat did not start: {}", Error::Timeout)),
                cx,
            );
        }
        let request = starting.request.clone();
        let Some(result) = self.take_script_response(&request) else {
            return;
        };
        let Some(starting) = self.chats.starting.take() else {
            return;
        };
        let typed = result
            .map_err(|error| super::Error::from(Error::Request(error)).into())
            .and_then(|response| {
                crate::worktree_scripts::created_tab(&response).map_err(|error| match error {
                    crate::Error::WorktreeScriptsResponse => {
                        super::Error::from(Error::Response).into()
                    }
                    error => error,
                })
            })
            .and_then(|(tab, pane)| {
                let line = if starting.remote {
                    remote_command_line(starting.agent)
                } else {
                    command_line(starting.agent)
                };
                self.type_script_line(&starting.boot, &pane, &line)?;
                Ok(tab)
            });
        match typed {
            Ok(tab) => {
                self.navigate_endpoint(&starting.endpoint_id, NavigationTarget::Tab(&tab), cx);
                cx.notify();
            }
            Err(error) => self.show_flash(
                Flash::warning(format!("The chat did not start: {error}")),
                cx,
            ),
        }
    }
}

//! Starting a closed agent again: a new tab in its workspace, in the folder
//! it ran from, with the agent's command typed into the tab's shell. The
//! endpoint API has no method that starts a command, so this follows the
//! worktree scripts: `tab.create` through the connection's script slot, then
//! one line typed into the pane it made. The line comes from the agent kinds
//! this client knows, never from the daemon's text.

use super::closed;
use crate::{HerdrWindow, NavigationTarget, teleport::AgentKind, window::Flash};
use gpui::Context;
use herdr_client::Method;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a reopen may wait for its host to come up and answer.
const DEADLINE: Duration = Duration::from_secs(15);

/// The line that starts `kind` again, picking up the conversation it last
/// had in that folder where the agent offers that; the rest start afresh.
pub(crate) fn command_line(kind: AgentKind) -> String {
    let resume: &[&str] = match kind {
        AgentKind::Claude | AgentKind::Opencode => &["--continue"],
        AgentKind::Codex => &["resume", "--last"],
        _ => &[],
    };
    std::iter::once(kind.binary())
        .chain(resume.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The kind a closed agent can be started again as, if this client knows it.
pub(crate) fn kind(agent: &closed::Agent) -> Option<AgentKind> {
    AgentKind::parse(agent.row.identity.as_deref()?)
}

/// A reopen on its way.
#[derive(Debug)]
pub(crate) struct Job {
    /// The boot that owns the closed agent's saved workspace ID.
    boot: String,
    /// The closed agent's id.
    closed: u64,
    endpoint: String,
    workspace: String,
    cwd: Option<String>,
    line: String,
    deadline: Instant,
    /// The `tab.create` request id.
    sent: Option<String>,
}

impl Job {
    fn new(agent: &closed::Agent, kind: AgentKind, now: Instant) -> Option<Self> {
        let super::Open::Pane { workspace, .. } = &agent.row.open else {
            return None;
        };
        Some(Self {
            boot: agent.boot.clone(),
            closed: agent.id,
            endpoint: agent.row.endpoint.clone(),
            workspace: workspace.clone(),
            cwd: agent.cwd.clone(),
            line: command_line(kind),
            deadline: now + DEADLINE,
            sent: None,
        })
    }

    pub(crate) fn create_params(&self) -> Value {
        let mut params = json!({"workspace_id": self.workspace, "focus": true});
        if let Some(cwd) = &self.cwd {
            params["cwd"] = Value::String(cwd.clone());
        }
        params
    }
}

impl HerdrWindow {
    /// Starts the closed agent `id` again, switching to its host first.
    pub(crate) fn reopen_closed_agent(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(agent) = self.overview.closed.get(id) else {
            return;
        };
        let Some(job) = kind(agent).and_then(|kind| Job::new(agent, kind, Instant::now())) else {
            return;
        };
        if self.script_slot_busy() {
            self.show_flash(Flash::warning("Another tab is still opening"), cx);
            return;
        }
        if self.endpoints[self.selected_endpoint].id != job.endpoint
            && (self.menu.page.is_some() || !self.select_endpoint(&job.endpoint, cx))
        {
            self.show_flash(Flash::warning("Cannot switch to the agent's host"), cx);
            return;
        }
        self.overview.reopen = Some(job);
        self.poll_overview_reopen(cx);
    }

    /// Advances a reopen on every tick: sends its `tab.create` once its host
    /// is the shown one, then types the agent's line into the new pane.
    pub(super) fn poll_overview_reopen(&mut self, cx: &mut Context<Self>) {
        let Some(job) = &self.overview.reopen else {
            return;
        };
        if Instant::now() >= job.deadline {
            return self.fail_overview_reopen("The agent's host did not answer in time", cx);
        }
        if self.endpoints[self.selected_endpoint].id != job.endpoint {
            return;
        }
        let Some(boot) = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .map(|snapshot| snapshot.boot_id.clone())
        else {
            return;
        };
        if job.boot != boot {
            return self.fail_overview_reopen(crate::Error::StaleConnection, cx);
        }
        let Some(request) = job.sent.clone() else {
            let open = self.live.snapshot.as_deref().is_some_and(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == job.workspace)
            });
            if !open {
                return self.fail_overview_reopen("The agent's workspace is closed", cx);
            }
            if self.script_job_busy() {
                return;
            }
            let params = job.create_params();
            let sent = self.endpoints[self.selected_endpoint]
                .connection
                .request_script(&boot, Method::TabCreate, params);
            match sent {
                Ok(request) => {
                    if let Some(job) = &mut self.overview.reopen {
                        job.sent = Some(request);
                    }
                }
                Err(error) => self.fail_overview_reopen(error, cx),
            }
            return;
        };
        let Some(result) = self.take_script_response(&request) else {
            return;
        };
        let created = result
            .map_err(|error| super::Error::ReopenRequest(error).into())
            .and_then(|response| {
                crate::worktree_scripts::created_tab(&response).map_err(|error| match error {
                    crate::Error::WorktreeScripts(crate::worktree_scripts::Error::Response) => {
                        super::Error::ReopenResponse.into()
                    }
                    error => error,
                })
            });
        let Some(job) = self.overview.reopen.take() else {
            return;
        };
        let typed = created.and_then(|(tab, pane)| {
            self.type_script_line(&boot, &pane, &job.line)?;
            Ok(tab)
        });
        match typed {
            Ok(tab) => {
                self.overview.closed.reopened(job.closed);
                self.navigate_endpoint(&job.endpoint, NavigationTarget::Tab(&tab), cx);
                cx.notify();
            }
            Err(error) => self.fail_overview_reopen(error, cx),
        }
    }

    fn fail_overview_reopen(&mut self, why: impl std::fmt::Display, cx: &mut Context<Self>) {
        self.overview.reopen = None;
        self.show_flash(
            Flash::warning(format!("The agent did not reopen: {why}")),
            cx,
        );
    }
}

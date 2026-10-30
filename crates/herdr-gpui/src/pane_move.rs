//! Moving a pane to another tab, a new tab, or a new workspace.
//!
//! `pane.move` is not among the methods Herdr advertises to GUI clients, so
//! the move runs `herdr pane move` against the endpoint's session, locally or
//! over SSH, through the same host scripts Teleport uses. The CLI's exit and
//! envelope are authoritative; the snapshot that follows shows the result.

use crate::i18n::fl;
use crate::teleport::Host;
use herdr_client::protocol::{ClientShellSnapshot, ClientShellWorkspace};
use serde::Deserialize;
use std::sync::atomic::AtomicBool;

/// Where a pane goes. Every variant names its target explicitly, so the move
/// never depends on which pane or workspace the daemon has focused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Destination {
    /// Split the tab's focused pane, placing the moved pane on its right.
    Tab { tab_id: String },
    /// A new tab at the end of this workspace.
    NewTab { workspace_id: String },
    /// A new workspace holding only the moved pane.
    NewWorkspace,
}

/// One destination row offered for a pane, captured from a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) destination: Destination,
    /// The workspace holding the destination, when it is not the pane's own
    /// place. Kept apart from `place` so a long workspace name can be cut
    /// short without hiding which tab the row names.
    pub(crate) workspace: Option<String>,
    /// The tab, or "New Tab"/"New Workspace".
    pub(crate) place: String,
}

/// The existing tabs a pane in `tab_id` can join, plus a new tab in every
/// other workspace, in sidebar order. The pane's own tab is left out: Herdr
/// refuses a move into the tab the pane is already in.
pub(crate) fn choices(snapshot: &ClientShellSnapshot, tab_id: &str) -> Vec<Choice> {
    let source = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == tab_id)
        .map(|tab| tab.workspace_id.as_str());
    let mut choices = Vec::new();
    for workspace in &snapshot.workspaces {
        let mut tabs: Vec<_> = snapshot
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace.workspace_id && tab.tab_id != tab_id)
            .collect();
        tabs.sort_by_key(|tab| tab.number);
        choices.extend(tabs.into_iter().map(|tab| Choice {
            destination: Destination::Tab {
                tab_id: tab.tab_id.clone(),
            },
            workspace: Some(workspace.label.clone()),
            place: tab.label.clone(),
        }));
        if source != Some(workspace.workspace_id.as_str()) {
            choices.push(Choice {
                destination: Destination::NewTab {
                    workspace_id: workspace.workspace_id.clone(),
                },
                workspace: Some(workspace.label.clone()),
                place: fl!("pane-menu-new-tab"),
            });
        }
    }
    choices
}

/// The workspaces a tab in `workspace_id` can move to: a new workspace first,
/// then a new tab in each other workspace, in sidebar order.
pub(crate) fn workspace_choices(snapshot: &ClientShellSnapshot, workspace_id: &str) -> Vec<Choice> {
    let order: Vec<&ClientShellWorkspace> = crate::sidebar::workspace_entries(&snapshot.workspaces)
        .into_iter()
        .map(|(index, _)| &snapshot.workspaces[index])
        .collect();
    std::iter::once(Choice {
        destination: Destination::NewWorkspace,
        workspace: None,
        place: fl!("pane-menu-new-workspace"),
    })
    .chain(
        order
            .into_iter()
            .filter(|workspace| workspace.workspace_id != workspace_id)
            .map(|workspace| Choice {
                destination: Destination::NewTab {
                    workspace_id: workspace.workspace_id.clone(),
                },
                workspace: None,
                place: workspace.label.clone(),
            }),
    )
    .collect()
}

impl Destination {
    /// Whether this destination still exists in `snapshot`.
    pub(crate) fn exists(&self, snapshot: &ClientShellSnapshot) -> bool {
        match self {
            Self::Tab { tab_id } => snapshot.tabs.iter().any(|tab| tab.tab_id == *tab_id),
            Self::NewTab { workspace_id } => snapshot
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == *workspace_id),
            Self::NewWorkspace => true,
        }
    }
}

/// Why Herdr left a pane where it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unchanged {
    SameTab,
    ZoomedTab,
    /// A reason this client does not know yet.
    #[serde(other)]
    Other,
}

impl std::fmt::Display for Unchanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SameTab => "The pane is already in that tab.",
            Self::ZoomedTab => "A zoomed tab cannot give or take panes. Unzoom it and try again.",
            Self::Other => "Herdr left the pane where it was.",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Moving panes needs a local session or SSH host; custom socket endpoints are unsupported."
    )]
    UnsupportedHost,
    /// `herdr --session NAME` always addresses the release installation, so
    /// a development session's panes cannot be reached by name: the command
    /// would act on the release session that shares it.
    #[error("Moving panes is not available for development (--dev) sessions.")]
    DevelopmentSession,
    /// An ID the CLI would read as an option or that cannot be one at all.
    #[error("Herdr ID {0:?} cannot be passed to the herdr CLI.")]
    InvalidId(String),
    #[error("{0}")]
    Unchanged(Unchanged),
    /// Herdr answered with its own error envelope.
    #[error("Could not move the pane: {message}")]
    Refused { code: String, message: String },
    #[error("Could not move the pane: the herdr CLI was not found on this host.")]
    MissingCli(#[source] herdr_client::Error),
    #[error("Could not move the pane: {0}")]
    Script(#[source] herdr_client::Error),
    #[error("Could not move the pane: herdr returned unexpected output.")]
    Decode(#[source] serde_json::Error),
    #[error("Could not move pane {pane_id} into the created tab: {source}")]
    PartialTabMove {
        pane_id: String,
        created_tab_id: String,
        #[source]
        source: Box<Error>,
    },
    /// Every pane moved, but the created tab did not keep the source name.
    #[error("Moved the tab, but could not keep its name: {source}")]
    Rename {
        tab_id: String,
        #[source]
        source: Box<Error>,
    },
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// A pane move ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pane_id: String,
    destination: Destination,
}

/// A move of every pane in a tab to a workspace destination. The first pane
/// opens the new tab or workspace; any remaining panes join that tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabRequest {
    first: Request,
    rest: Vec<String>,
    tab_label: Option<String>,
}

/// A finished tab move. Every pane moved; `warning` holds what was left
/// undone, so the caller can say so instead of reporting a clean move.
#[derive(Debug, Default)]
pub(crate) struct TabMoved {
    pub(crate) warning: Option<Error>,
}

/// An ID the CLI takes as one positional or option value: nonempty, never
/// read as an option, and free of control characters.
fn checked(id: &str) -> Result<&str> {
    if id.is_empty() || id.starts_with('-') || id.chars().any(char::is_control) {
        return Err(Error::InvalidId(id.to_owned()));
    }
    Ok(id)
}

impl Request {
    pub(crate) fn new(pane_id: String, destination: Destination) -> Result<Self> {
        checked(&pane_id)?;
        match &destination {
            Destination::Tab { tab_id: id } | Destination::NewTab { workspace_id: id } => {
                checked(id)?;
            }
            Destination::NewWorkspace => {}
        }
        Ok(Self {
            pane_id,
            destination,
        })
    }

    /// The `herdr` arguments for this move. Focus follows the pane, as it does
    /// for a move made from the Herdr TUI.
    fn args(&self) -> Vec<&str> {
        let mut args = vec!["pane", "move", self.pane_id.as_str()];
        match &self.destination {
            Destination::Tab { tab_id } => {
                args.extend(["--tab", tab_id.as_str(), "--split", "right"]);
            }
            Destination::NewTab { workspace_id } => {
                args.extend(["--new-tab", "--workspace", workspace_id.as_str()]);
            }
            Destination::NewWorkspace => args.push("--new-workspace"),
        }
        args.push("--focus");
        args
    }

    /// Run the move on `host`. Blocking: call only on a background worker.
    pub(crate) fn run(&self, host: &Host, cancelled: &AtomicBool) -> Result<()> {
        let output = host
            .capture(&Host::herdr_line(&self.args()), cancelled)
            .map_err(failure)?;
        outcome(&output)
    }
}

impl TabRequest {
    pub(crate) fn new(
        panes: Vec<String>,
        destination: Destination,
        tab_label: Option<String>,
    ) -> Result<Self> {
        let mut panes = panes.into_iter();
        let first = panes.next().unwrap_or_default();
        let first = Request::new(first, destination)?;
        let rest: Vec<String> = panes
            .map(|id| checked(&id).map(str::to_owned))
            .collect::<Result<_>>()?;
        let tab_label = match tab_label {
            Some(label) => {
                let trimmed = label.trim();
                if trimmed.chars().any(char::is_control) {
                    return Err(Error::InvalidId(label));
                }
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            }
            None => None,
        };
        Ok(Self {
            first,
            rest,
            tab_label,
        })
    }

    /// Run the tab move on `host`. Blocking: call only on a background worker.
    /// A failed rename does not stop the remaining panes, which would leave
    /// the tab split across workspaces; it comes back as the warning.
    pub(crate) fn run(&self, host: &Host, cancelled: &AtomicBool) -> Result<TabMoved> {
        let output = host
            .capture(&Host::herdr_line(&self.first.args()), cancelled)
            .map_err(failure)?;
        let tab_id = outcome_tab(&output)?;
        let tab_id = tab_id.ok_or_else(|| Error::InvalidId(String::new()))?;
        let warning = self.tab_label.as_ref().and_then(|tab_label| {
            host.capture(
                &Host::herdr_line(&["tab", "rename", tab_id.as_str(), tab_label.as_str()]),
                cancelled,
            )
            .err()
            .map(|source| Error::Rename {
                tab_id: tab_id.clone(),
                source: Box::new(failure(source)),
            })
        });
        for pane_id in &self.rest {
            Request::new(
                pane_id.clone(),
                Destination::Tab {
                    tab_id: tab_id.clone(),
                },
            )?
            .run(host, cancelled)
            .map_err(|source| Error::PartialTabMove {
                pane_id: pane_id.clone(),
                created_tab_id: tab_id.clone(),
                source: Box::new(source),
            })?;
        }
        Ok(TabMoved { warning })
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    result: T,
}

#[derive(Deserialize)]
struct MoveResponse {
    move_result: Outcome,
}

#[derive(Deserialize)]
struct Outcome {
    changed: bool,
    #[serde(default)]
    reason: Option<Unchanged>,
    #[serde(default)]
    pane: Option<MovedTab>,
    #[serde(default)]
    created_tab: Option<MovedTab>,
}

#[derive(Deserialize)]
struct MovedTab {
    tab_id: String,
}

fn outcome(output: &[u8]) -> Result<()> {
    outcome_tab(output).map(|_| ())
}

fn outcome_tab(output: &[u8]) -> Result<Option<String>> {
    let outcome = serde_json::from_slice::<Envelope<MoveResponse>>(output)
        .map_err(Error::Decode)?
        .result
        .move_result;
    if outcome.changed {
        let tab_id = outcome.created_tab.or(outcome.pane).map(|tab| tab.tab_id);
        return Ok(tab_id);
    }
    Err(Error::Unchanged(outcome.reason.unwrap_or(Unchanged::Other)))
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code: String,
    message: String,
}

/// Classify a failed script: Herdr's own error envelope on stderr becomes a
/// [`Error::Refused`] with its message; a shell that could not find `herdr`
/// says so; anything else keeps the script error.
fn failure(error: herdr_client::Error) -> Error {
    let herdr_client::Error::ScriptExit { status, stderr } = &error else {
        return Error::Script(error);
    };
    // The CLI prints its envelope as the last stderr line.
    if let Some(ErrorEnvelope {
        error: ErrorBody { code, message },
    }) = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str(line.trim()).ok())
    {
        return Error::Refused { code, message };
    }
    if status.code() == Some(127) {
        return Error::MissingCli(error);
    }
    Error::Script(error)
}

/// The Teleport host for `target`, which this move shares.
pub(crate) fn host_for(target: &herdr_client::ConnectTarget) -> Result<Host> {
    // Teleport's hosts refuse what its scripts cannot reach: a custom socket
    // has no host, and the CLI cannot name a development session.
    crate::teleport::host_for(target).map_err(|error| match error {
        crate::teleport::Error::DevelopmentSession => Error::DevelopmentSession,
        _ => Error::UnsupportedHost,
    })
}

/// Why panes on `target` cannot move, if they cannot: host scripts need a
/// POSIX client (see `herdr_client::run_script`) and a host the `herdr` CLI
/// can address.
pub(crate) fn refusal(target: &herdr_client::ConnectTarget) -> Option<Error> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Some(Error::UnsupportedHost);
    }
    host_for(target).err()
}

#[cfg(test)]
mod tests;

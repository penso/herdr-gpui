//! Moving a pane to another tab, a new tab, or a new workspace.
//!
//! `pane.move` is not among the methods Herdr advertises to GUI clients, so
//! the move runs `herdr pane move` against the endpoint's session, locally or
//! over SSH, through the same host scripts Teleport uses. The CLI's exit and
//! envelope are authoritative; the snapshot that follows shows the result.

use crate::teleport::Host;
use herdr_client::protocol::ClientShellSnapshot;
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
    pub(crate) label: String,
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
            label: format!("{} \u{203a} {}", workspace.label, tab.label),
        }));
        if source != Some(workspace.workspace_id.as_str()) {
            choices.push(Choice {
                destination: Destination::NewTab {
                    workspace_id: workspace.workspace_id.clone(),
                },
                label: format!("{} \u{203a} New Tab", workspace.label),
            });
        }
    }
    choices
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
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// A pane move ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pane_id: String,
    destination: Destination,
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
        let output = host.cli_output(&self.args(), cancelled).map_err(failure)?;
        outcome(&output)
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
}

fn outcome(output: &[u8]) -> Result<()> {
    let outcome = serde_json::from_slice::<Envelope<MoveResponse>>(output)
        .map_err(Error::Decode)?
        .result
        .move_result;
    if outcome.changed {
        return Ok(());
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
    // Only a custom socket has no host; Teleport refuses it for the same reason.
    crate::teleport::host_for(target).map_err(|_| Error::UnsupportedHost)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use herdr_client::ConnectTarget;

    fn snapshot() -> ClientShellSnapshot {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let mut second = snapshot.tabs[0].clone();
        second.tab_id = "w1:t2".into();
        second.number = 2;
        second.label = "logs".into();
        let mut workspace = snapshot.workspaces[0].clone();
        workspace.workspace_id = "w2".into();
        workspace.label = "other".into();
        let mut remote = snapshot.tabs[0].clone();
        remote.tab_id = "w2:t1".into();
        remote.workspace_id = "w2".into();
        remote.label = "build".into();
        // Out of number order: the choices follow the tab numbers.
        snapshot.tabs.insert(0, second);
        snapshot.workspaces.push(workspace);
        snapshot.tabs.push(remote);
        snapshot
    }

    #[test]
    fn choices_skip_the_own_tab_and_offer_new_tabs_elsewhere() {
        let snapshot = snapshot();
        let choices = choices(&snapshot, "w1:t1");
        assert_eq!(
            choices,
            [
                Choice {
                    destination: Destination::Tab {
                        tab_id: "w1:t2".into()
                    },
                    label: "repo \u{203a} logs".into(),
                },
                Choice {
                    destination: Destination::Tab {
                        tab_id: "w2:t1".into()
                    },
                    label: "other \u{203a} build".into(),
                },
                Choice {
                    destination: Destination::NewTab {
                        workspace_id: "w2".into()
                    },
                    label: "other \u{203a} New Tab".into(),
                },
            ]
        );
        assert!(choices.iter().all(|c| c.destination.exists(&snapshot)));
        let mut gone = snapshot.clone();
        gone.tabs.retain(|tab| tab.workspace_id != "w2");
        gone.workspaces.retain(|w| w.workspace_id != "w2");
        assert!(!choices[1].destination.exists(&gone));
        assert!(!choices[2].destination.exists(&gone));
        assert!(Destination::NewWorkspace.exists(&gone));
        // A lone tab in a lone workspace has nowhere to go but new places.
        let lone: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        assert!(super::choices(&lone, "w1:t1").is_empty());
    }

    #[test]
    fn arguments_name_every_target_explicitly() {
        let tab = Request::new(
            "w1:p1".into(),
            Destination::Tab {
                tab_id: "w2:t1".into(),
            },
        )
        .unwrap();
        assert_eq!(
            tab.args(),
            [
                "pane", "move", "w1:p1", "--tab", "w2:t1", "--split", "right", "--focus"
            ]
        );
        let new_tab = Request::new(
            "w1:p1".into(),
            Destination::NewTab {
                workspace_id: "w1".into(),
            },
        )
        .unwrap();
        assert_eq!(
            new_tab.args(),
            [
                "pane",
                "move",
                "w1:p1",
                "--new-tab",
                "--workspace",
                "w1",
                "--focus"
            ]
        );
        let workspace = Request::new("w1:p1".into(), Destination::NewWorkspace).unwrap();
        assert_eq!(
            workspace.args(),
            ["pane", "move", "w1:p1", "--new-workspace", "--focus"]
        );
    }

    #[test]
    fn ids_the_cli_would_misread_are_refused() {
        for id in ["", "--new-workspace", "w1\np1"] {
            assert!(matches!(
                Request::new(id.into(), Destination::NewWorkspace),
                Err(Error::InvalidId(rejected)) if rejected == id
            ));
            assert!(matches!(
                Request::new("w1:p1".into(), Destination::Tab { tab_id: id.into() }),
                Err(Error::InvalidId(_))
            ));
            assert!(matches!(
                Request::new(
                    "w1:p1".into(),
                    Destination::NewTab {
                        workspace_id: id.into()
                    }
                ),
                Err(Error::InvalidId(_))
            ));
        }
    }

    #[test]
    fn outcomes_follow_the_envelope() {
        assert!(
            outcome(br#"{"id":"cli:pane:move","result":{"type":"pane_move","move_result":{"changed":true,"previous_pane_id":"w1:p1"}}}"#)
                .is_ok()
        );
        for (reason, expected) in [
            ("\"same_tab\"", Unchanged::SameTab),
            ("\"zoomed_tab\"", Unchanged::ZoomedTab),
            ("\"from_the_future\"", Unchanged::Other),
            ("null", Unchanged::Other),
        ] {
            let json = format!(
                r#"{{"result":{{"type":"pane_move","move_result":{{"changed":false,"reason":{reason}}}}}}}"#
            );
            assert!(matches!(
                outcome(json.as_bytes()),
                Err(Error::Unchanged(found)) if found == expected
            ));
        }
        assert!(matches!(outcome(b"not json"), Err(Error::Decode(_))));
        assert_eq!(
            Error::Unchanged(Unchanged::ZoomedTab).to_string(),
            "A zoomed tab cannot give or take panes. Unzoom it and try again."
        );
    }

    #[test]
    #[cfg(unix)]
    fn script_failures_keep_herdr_messages_and_sources() {
        use std::os::unix::process::ExitStatusExt;
        let exit = |code: i32, stderr: &str| herdr_client::Error::ScriptExit {
            status: std::process::ExitStatus::from_raw(code << 8),
            stderr: stderr.into(),
        };
        let refused = failure(exit(
            1,
            "warning\n{\"id\":\"cli:pane:move\",\"error\":{\"code\":\"tab_not_found\",\"message\":\"tab w9:t1 not found\"}}\n",
        ));
        assert!(matches!(
            &refused,
            Error::Refused { code, message } if code == "tab_not_found" && message == "tab w9:t1 not found"
        ));
        assert_eq!(
            refused.to_string(),
            "Could not move the pane: tab w9:t1 not found"
        );
        let missing = failure(exit(127, "sh: herdr: not found"));
        assert!(matches!(missing, Error::MissingCli(_)));
        assert!(std::error::Error::source(&missing).is_some());
        let usage = failure(exit(2, "usage: herdr pane move"));
        assert!(matches!(
            usage,
            Error::Script(herdr_client::Error::ScriptExit { .. })
        ));
        assert!(matches!(
            failure(herdr_client::Error::ScriptTimeout),
            Error::Script(herdr_client::Error::ScriptTimeout)
        ));
    }

    #[test]
    fn custom_sockets_have_no_host() {
        assert!(matches!(
            host_for(&ConnectTarget::Socket("/tmp/s".into())),
            Err(Error::UnsupportedHost)
        ));
        assert!(host_for(&ConnectTarget::Local).is_ok());
    }

    /// The real runner against a stand-in `herdr` that records its arguments:
    /// the session is addressed, every argument arrives as one word, and the
    /// envelope or exit decides the result.
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn runs_the_session_cli_with_quoted_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("herdr");
        std::fs::write(
            &fake,
            r#"#!/bin/sh
for arg in "$@"; do printf '%s\n' "$arg"; done > "$HOME/args"
case "$7" in
  w1:t2) printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true}}}' ;;
  w1:t1) printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":false,"reason":"same_tab"}}}' ;;
  *) printf '%s\n' '{"id":"cli:pane:move","error":{"code":"tab_not_found","message":"tab not found"}}' >&2; exit 1 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut host = host_for(&ConnectTarget::Session {
            name: "work".into(),
            development: false,
        })
        .unwrap();
        host.env = vec![
            ("HOME".into(), home.path().to_string_lossy().into_owned()),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ];
        let cancelled = AtomicBool::new(false);
        let to = |tab: &str| {
            Request::new("w1:p1".into(), Destination::Tab { tab_id: tab.into() }).unwrap()
        };
        to("w1:t2").run(&host, &cancelled).unwrap();
        assert_eq!(
            std::fs::read_to_string(home.path().join("args")).unwrap(),
            "--session\nwork\npane\nmove\nw1:p1\n--tab\nw1:t2\n--split\nright\n--focus\n"
        );
        assert!(matches!(
            to("w1:t1").run(&host, &cancelled),
            Err(Error::Unchanged(Unchanged::SameTab))
        ));
        assert!(matches!(
            to("w1:t 'x").run(&host, &cancelled),
            Err(Error::Refused { code, .. }) if code == "tab_not_found"
        ));
        assert!(
            std::fs::read_to_string(home.path().join("args"))
                .unwrap()
                .contains("\nw1:t 'x\n")
        );
    }
}

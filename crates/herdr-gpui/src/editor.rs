//! Opening a file at a line in the user's terminal editor, in a new pane
//! beside the one the user was working in, the way an IDE jumps to a
//! definition. The daemon owns the pane and the editor runs in it like any
//! other program, so closing the editor closes its pane.
//!
//! The endpoint API has no method that starts a command in a pane, so the
//! editor starts from the new pane's shell: `pane.split` opens it in the
//! source pane's directory, then one line is typed to run the editor. Paths
//! come from terminal output, which is untrusted, so the typed line holds
//! them only as single-quoted words without a quote, backslash, or control
//! character: POSIX shells, fish, and nushell all read such a word literally.
//! Anything else opens in the system's default application instead.
//!
//! When the editor is Neovim it listens on a private socket, and later
//! files for the same tab open in that pane (see [`nvim`]).

mod nvim;

use crate::{HerdrWindow, NavigationTarget};
use gpui::Context;
use herdr_client::{Method, protocol::ClientPaneInputEvent};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Whether this build starts the editor: the typed line runs POSIX `sh`,
/// which the shells of a Windows pane do not have.
pub(crate) const SUPPORTED: bool = cfg!(unix);

/// The longest command template accepted from the config.
const MAX_COMMAND_BYTES: usize = 1024;

/// Files a terminal editor has no use for, left to the system's viewer.
const MEDIA: [&str; 13] = [
    "bmp", "gif", "heic", "ico", "jpeg", "jpg", "mov", "mp3", "mp4", "pdf", "png", "svg", "webp",
];

/// Whether `text` reads the same as a single-quoted word in every shell the
/// typed line may reach.
pub(crate) fn quotable(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 4096
        && !text
            .chars()
            .any(|c| c == '\'' || c == '\\' || c.is_control())
}

/// The `editor_command` setting: how the editor is started, with `{file}`
/// and `{line}` standing for the file and the line to open it at. Without
/// either placeholder, `+{line} {file}` follows the command, which vi, Vim,
/// Neovim, Emacs, nano, micro, and Kakoune all understand.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct EditorCommand(String);

impl TryFrom<String> for EditorCommand {
    type Error = crate::Error;

    fn try_from(command: String) -> crate::Result<Self> {
        let command = command.trim();
        // The template is typed inside single quotes too.
        if command.len() > MAX_COMMAND_BYTES || !quotable(command) {
            return Err(crate::Error::EditorCommand);
        }
        Ok(Self(command.to_owned()))
    }
}

impl EditorCommand {
    /// The `sh` script that runs the editor, reading the file from `$0`, the
    /// line from `$1`, and the socket a Neovim listens on from `$2`. A
    /// command with placeholders is run as written.
    fn script(&self) -> String {
        let command = &self.0;
        if command.contains("{file}") || command.contains("{line}") {
            format!("exec {}", placed(command))
        } else if nvim::is_nvim(command) {
            format!(
                "[ -n \"$2\" ] && exec {command} --listen \"$2\" \"+$1\" \"$0\"; exec {command} \"+$1\" \"$0\""
            )
        } else {
            format!("exec {command} \"+$1\" \"$0\"")
        }
    }
}

/// `command` with its placeholders standing for `$0` and `$1`, each read as
/// one word with no pattern matching: quoted, or bare where the template
/// already quotes them, as in `subl "{file}:{line}"`. Quoting there again
/// would close the template's quotes and leave the path open to splitting.
fn placed(command: &str) -> String {
    let mut placed = String::with_capacity(command.len() + 8);
    let mut quoted = false;
    let mut rest = command;
    while let Some(c) = rest.chars().next() {
        let (word, len) = if rest.starts_with("{file}") {
            ("$0", "{file}".len())
        } else if rest.starts_with("{line}") {
            ("$1", "{line}".len())
        } else {
            quoted ^= c == '"';
            placed.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        if quoted {
            placed.push_str(word);
        } else {
            placed.push('"');
            placed.push_str(word);
            placed.push('"');
        }
        rest = &rest[len..];
    }
    placed
}

/// The pane's own editor, which listens on `$2` when it is Neovim.
const DEFAULT_SCRIPT: &str = r#"e=${VISUAL:-${EDITOR:-vi}}; case "${e##*/}" in nvim*) [ -n "$2" ] && exec $e --listen "$2" "+$1" "$0";; esac; exec $e "+$1" "$0""#;

/// A file to open, at a line when one is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorTarget {
    /// Absolute, and checked to exist on this machine.
    pub(crate) path: PathBuf,
    pub(crate) line: Option<u32>,
}

impl EditorTarget {
    /// Whether a terminal editor is the place for `path`: a regular file
    /// that is not an image, a video, or a PDF.
    pub(crate) fn editable(path: &Path, metadata: &std::fs::Metadata) -> bool {
        let media = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|extension| MEDIA.contains(&extension.as_str()));
        metadata.is_file() && !media
    }
}

/// The line typed into the new pane's shell. A leading space keeps it out of
/// the history of shells that honor that, and `exec` hands the pane to the
/// editor, so quitting the editor closes the pane. Without a configured
/// command, the pane's own `$VISUAL` or `$EDITOR` is used, then `vi`. A
/// Neovim listens on `socket`, whose private folder the line makes.
pub(crate) fn command_line(
    target: &EditorTarget,
    command: Option<&EditorCommand>,
    socket: Option<&Path>,
) -> crate::Result<String> {
    if !SUPPORTED {
        return Err(crate::Error::EditorUnsupported);
    }
    let path = target
        .path
        .to_str()
        .filter(|path| quotable(path) && target.path.is_absolute())
        .ok_or(crate::Error::EditorPath)?;
    let line = target.line.unwrap_or(1).max(1);
    let script = command.map_or_else(|| DEFAULT_SCRIPT.to_owned(), EditorCommand::script);
    let Some(socket) = socket
        .and_then(Path::to_str)
        .filter(|socket| quotable(socket))
    else {
        return Ok(format!(" exec sh -c '{script}' '{path}' {line}"));
    };
    Ok(format!(
        " exec sh -c 'mkdir -p -m 700 \"${{2%/*}}\" || set -- \"$1\" \"\"; {script}' '{path}' {line} '{socket}'"
    ))
}

/// An editor pane this window started with a Neovim socket, which later
/// files for its tab open in while the pane lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorPane {
    endpoint_id: String,
    boot: String,
    pane_id: String,
    socket: PathBuf,
}

/// Editor panes remembered at most; the oldest is forgotten first.
const MAX_EDITOR_PANES: usize = 16;

/// One editor pane being opened: the split it waits on and what to type
/// into the pane the split makes. Fenced like a worktree script, so a reply
/// from a replaced connection never types into anything.
pub(crate) struct Job {
    endpoint: (u64, u64),
    endpoint_id: String,
    boot: String,
    request: String,
    line: String,
    socket: Option<PathBuf>,
}

/// The new pane a `pane.split` response names.
fn created_pane(response: &Value) -> crate::Result<String> {
    if let Some(error) = response.get("error").filter(|error| !error.is_null()) {
        return Err(crate::Error::DaemonResponse(error.clone()));
    }
    response["result"]["pane"]["pane_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or(crate::Error::EditorResponse)
}

impl HerdrWindow {
    /// Opens `target` in the editor, in a pane split to the right of
    /// `beside`, or of the focused pane. Reports a failure as a flash.
    pub(crate) fn open_in_editor(
        &mut self,
        target: &EditorTarget,
        beside: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if let Some(reused) = self.reusable_editor(beside) {
            self.reuse_editor(reused, target.clone(), beside.map(str::to_owned), cx);
            return;
        }
        if let Err(error) = self.start_editor(target, beside) {
            self.editor_failed(target, error, cx);
        }
        cx.notify();
    }

    /// Reports why `target` did not open in the editor. A path no shell
    /// line can carry opens in the system's default app instead, as
    /// [`crate::Error::EditorPath`] says, under the rule a clicked path
    /// follows, so a listed file is never run.
    fn editor_failed(
        &mut self,
        target: &EditorTarget,
        error: crate::Error,
        cx: &mut Context<Self>,
    ) {
        if matches!(error, crate::Error::EditorPath) {
            self.open_in_system_app(target.path.clone(), cx);
        }
        self.show_flash(crate::window::Flash::warning(error.to_string()), cx);
    }

    /// The editor pane started for the tab of `beside`, or of the focused
    /// pane, while it is still open. Panes that closed are forgotten.
    fn reusable_editor(&mut self, beside: Option<&str>) -> Option<EditorPane> {
        let snapshot = self
            .live
            .snapshot
            .clone()
            .filter(|_| self.live.status.is_connected())?;
        let endpoint = &self.endpoints[self.selected_endpoint].id;
        self.editor_panes.retain(|editor| {
            editor.boot != snapshot.boot_id
                || &editor.endpoint_id != endpoint
                || snapshot
                    .panes
                    .iter()
                    .any(|pane| pane.pane_id == editor.pane_id)
        });
        let tab = beside
            .or(snapshot.focused_pane_id.as_deref())
            .and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id))?
            .tab_id
            .clone();
        self.editor_panes
            .iter()
            .rev()
            .find(|editor| {
                &editor.endpoint_id == endpoint
                    && editor.boot == snapshot.boot_id
                    && snapshot
                        .panes
                        .iter()
                        .any(|pane| pane.pane_id == editor.pane_id && pane.tab_id == tab)
            })
            .cloned()
    }

    /// Sends `target` to the Neovim in `editor` off the UI thread and brings
    /// its pane forward; if that Neovim has gone, the pane is forgotten and
    /// the file opens in a new one. A Neovim still there but busy keeps its
    /// pane, which comes forward so its prompt can be answered.
    fn reuse_editor(
        &mut self,
        editor: EditorPane,
        target: EditorTarget,
        beside: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let socket = editor.socket.clone();
        let opened = cx.background_executor().spawn({
            let target = target.clone();
            async move { nvim::open(&socket, &target) }
        });
        cx.spawn(async move |this, cx| {
            let opened = opened.await;
            let _ = this.update(cx, |this, cx| {
                let gone = match opened {
                    Ok(()) => false,
                    Err(crate::Error::EditorRemote | crate::Error::EditorRemoteLaunch(_)) => true,
                    Err(error) => {
                        this.editor_failed(&target, error, cx);
                        false
                    }
                };
                if !gone {
                    this.navigate_endpoint(
                        &editor.endpoint_id,
                        NavigationTarget::Pane(&editor.pane_id),
                        cx,
                    );
                    return;
                }
                this.editor_panes.retain(|kept| *kept != editor);
                if let Err(error) = this.start_editor(&target, beside.as_deref()) {
                    this.editor_failed(&target, error, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_editor(&mut self, target: &EditorTarget, beside: Option<&str>) -> crate::Result<()> {
        if self.editor_open.is_some() && self.editor_current() {
            return Err(crate::Error::EditorBusy);
        }
        let socket = nvim::new_socket();
        let line = command_line(
            target,
            self.config.editor_command.as_ref(),
            socket.as_deref(),
        )?;
        if self.selected_is_remote() {
            return Err(crate::Error::EditorNoPane);
        }
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .ok_or(crate::Error::NotConnected)?;
        let pane = beside
            .or(snapshot.focused_pane_id.as_deref())
            .and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id))
            .ok_or(crate::Error::EditorNoPane)?;
        let cwd = pane
            .foreground_cwd
            .clone()
            .or_else(|| pane.cwd.clone())
            .or_else(|| {
                target
                    .path
                    .parent()
                    .and_then(Path::to_str)
                    .map(str::to_owned)
            });
        let mut params = json!({
            "target_pane_id": pane.pane_id,
            "direction": "right",
            "focus": true,
        });
        if let Some(cwd) = cwd {
            params["cwd"] = json!(cwd);
        }
        let boot = snapshot.boot_id.clone();
        let endpoint = &self.endpoints[self.selected_endpoint];
        let request = endpoint
            .connection
            .request_editor(&boot, Method::PaneSplit, params)?;
        self.editor_open = Some(Job {
            endpoint: (self.selection_epoch, endpoint.generation),
            endpoint_id: endpoint.id.clone(),
            boot,
            request,
            line,
            socket,
        });
        Ok(())
    }

    fn editor_current(&self) -> bool {
        self.editor_open.as_ref().is_some_and(|job| {
            job.endpoint
                == (
                    self.selection_epoch,
                    self.endpoints[self.selected_endpoint].generation,
                )
                && self.live.status.is_connected()
                && self
                    .live
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.boot_id == job.boot)
        })
    }

    /// Runs every window tick: once the split answers, types the editor's
    /// line into the new pane and brings its tab forward.
    pub(crate) fn poll_editor_open(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.editor_open.as_ref().map(|job| job.request.clone()) else {
            return;
        };
        if !self.editor_current() {
            self.editor_open = None;
            return;
        }
        let Some((id, Some(result))) = &self.live.editor_response else {
            return;
        };
        if *id != request {
            return;
        }
        let result = result.clone();
        self.live.editor_response = None;
        if let Ok(mut inbox) = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .try_lock()
            && inbox
                .editor_response
                .as_ref()
                .is_some_and(|(id, _)| *id == request)
        {
            inbox.editor_response = None;
        }
        let Some(job) = self.editor_open.take() else {
            return;
        };
        let typed = result
            .map_err(crate::Error::EditorRequest)
            .and_then(|response| created_pane(&response))
            .and_then(|pane| {
                let handle = self.endpoints[self.selected_endpoint]
                    .connection
                    .handle
                    .as_ref()
                    .ok_or(crate::Error::NotConnected)?;
                handle.send_input(
                    &job.boot,
                    &pane,
                    [
                        ClientPaneInputEvent::TextCommit(job.line.clone()),
                        crate::menu::enter_key(),
                    ],
                )?;
                Ok(pane)
            });
        match typed {
            Ok(pane) => {
                // Remembered whatever the editor turns out to be: a socket
                // nothing listens on is found out, and forgotten, on reuse.
                if let Some(socket) = job.socket {
                    if self.editor_panes.len() == MAX_EDITOR_PANES {
                        self.editor_panes.remove(0);
                    }
                    self.editor_panes.push(EditorPane {
                        endpoint_id: job.endpoint_id.clone(),
                        boot: job.boot.clone(),
                        pane_id: pane.clone(),
                        socket,
                    });
                }
                self.navigate_endpoint(&job.endpoint_id, NavigationTarget::Pane(&pane), cx);
            }
            Err(error) => {
                self.show_flash(
                    crate::window::Flash::warning(format!("The editor did not open: {error}")),
                    cx,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;

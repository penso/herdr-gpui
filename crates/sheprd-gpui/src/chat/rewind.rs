//! "Edit from here": a prompt of the user's goes back in the composer, and
//! sending it takes the conversation back to before it, with or without the
//! checkout's files.
//!
//! The files go first, so the agent never starts on the edit in a checkout
//! still being put back; a restore that fails leaves the conversation alone.
//! The draft stays in the composer until the program answers, as a refused
//! rewind changes nothing.

use super::{ChatInput, chats::ChatPane};
use crate::{
    SheprdWindow,
    checkpoint::{Checkout, Found, Origin, host_for},
    i18n::{Localize, fl},
    multiline_input::MultilineInput,
    window::chrome_focus::Region,
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{
    Attachment, Command, EntryId, EntryKind, Rejection, Transcript, assist,
};
use std::path::PathBuf;

/// What happens to the checkout's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Files {
    /// Only the conversation goes back.
    Keep,
    /// The checkout goes back to the checkpoint from the prompt's turn start.
    Restore,
}

/// The checkpoint from the start of the prompt's turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Checkpoint {
    Looking,
    Found(String),
    /// None was taken, or it has been pruned or is on another branch.
    Missing,
    /// The chat has no checkout this client can run Git on, or Git failed.
    Unreachable,
}

/// How far sending has gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Editing,
    /// The files are going back; the prompt goes out next.
    Restoring {
        attachments: Vec<Attachment>,
        text: String,
        /// Names this restore among the checkout's.
        ticket: u64,
    },
    /// The program has the command and has not answered.
    Sending,
}

pub(crate) struct Rewind {
    /// The prompt being edited.
    pub(crate) before: EntryId,
    pub(crate) checkout: Option<Checkout>,
    pub(crate) checkpoint: Checkpoint,
    /// What the composer held, put back on cancelling.
    draft: (String, Vec<PathBuf>),
    pub(crate) files: Files,
    pub(crate) focus: [FocusHandle; 2],
    /// The prompt carried files this run does not know the paths of.
    pub(crate) lost_files: bool,
    origin: Option<Origin>,
    pub(crate) stage: Stage,
    /// The workspace the chat's pane is in.
    pub(crate) workspace: Option<String>,
}

/// Stable across runs, unlike a hasher's random keys: the name is written
/// in checkpoints and read back by later runs.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The name a checkpoint of the prompt `id` goes by: its place among all the
/// chat's prompts, those dropped from the transcript included, and what it
/// says. Entry ids are no use, as a reopened chat numbers its log afresh, and
/// a chat's session id changes as it goes back. A log read only in part has
/// no places to give: an unnamed prompt can't be restored, while a misnumbered
/// one could restore the wrong files.
pub(crate) fn origin(pane: &str, transcript: &Transcript, id: EntryId) -> Option<Origin> {
    if transcript.prompts_uncounted {
        return None;
    }
    let mut place = transcript.dropped_prompts;
    for entry in &transcript.entries {
        let EntryKind::User { text, files } = &entry.kind else {
            continue;
        };
        place += 1;
        if entry.id == id {
            let typed = assist::typed(text, *files);
            return Some(Origin {
                chat: pane.to_owned(),
                prompt: format!("{place}-{:016x}", fnv(typed)),
            });
        }
    }
    None
}

/// The running turn's prompt, which a checkpoint taken now starts. A turn
/// this client has not heard of yet names nothing, rather than the prompt
/// before it: an unnamed checkpoint can't be restored from a prompt, while
/// a wrongly named one would restore the wrong files.
pub(crate) fn running_origin(pane: &str, transcript: &Transcript) -> Option<Origin> {
    transcript.turn?;
    let prompt = transcript
        .entries
        .iter()
        .rfind(|entry| matches!(entry.kind, EntryKind::User { .. }))?;
    origin(pane, transcript, prompt.id)
}

impl SheprdWindow {
    /// The workspace of a pane, and the checkout it shows when checkpoints
    /// can reach it.
    fn chat_checkout(&self, pane_id: &str) -> (Option<String>, Option<Checkout>) {
        let Some(snapshot) = self.live.snapshot.as_ref() else {
            return (None, None);
        };
        let Some(workspace) = snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane_id)
            .and_then(|agent| {
                snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == agent.workspace_id)
            })
        else {
            return (None, None);
        };
        let checkout = self
            .endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| host_for(&endpoint.connection.target, self.live.local_daemon_peer))
            .and_then(|host| {
                Checkout::new(
                    host,
                    &workspace.worktree.as_ref()?.key,
                    workspace.branch.as_deref(),
                )
            });
        (Some(workspace.workspace_id.clone()), checkout)
    }

    /// Puts prompt `before` in the composer to be edited, and looks for its
    /// checkpoint so the choice can say whether its files can come back.
    pub(super) fn begin_chat_rewind(
        &mut self,
        pane_id: &str,
        before: EntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (workspace, checkout) = self.chat_checkout(pane_id);
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let transcript = pane.session.transcript();
        if !transcript
            .session
            .as_ref()
            .is_some_and(|session| session.rewind)
            || pane.input() != ChatInput::Composer
            || pane.rewind_submitted()
        {
            return;
        }
        let Some(EntryKind::User { text, files }) =
            transcript.entry(before).map(|entry| &entry.kind)
        else {
            return;
        };
        let typed = assist::typed(text, *files).to_owned();
        let carried: Vec<PathBuf> = transcript
            .prompt_files
            .iter()
            .find(|known| known.id == before)
            .map(|known| {
                known
                    .attachments
                    .iter()
                    .map(|file| file.path.clone().into())
            })
            .into_iter()
            .flatten()
            .collect();
        let lost_files = carried.len() < *files;
        let origin = origin(pane_id, transcript, before);
        let Some(input) = pane.composer.clone() else {
            return;
        };
        // A second prompt replaces the first, and cancelling still gets back
        // what the user was writing before either.
        let draft = match pane.rewind.take() {
            Some(rewind) => rewind.draft,
            None => (
                input.read(cx).text().to_owned(),
                std::mem::take(&mut pane.attachments),
            ),
        };
        pane.attachments = carried;
        pane.error = None;
        let checkpoint = match (&checkout, &origin) {
            (None, _) => Checkpoint::Unreachable,
            (Some(_), None) => Checkpoint::Missing,
            (Some(_), Some(_)) => Checkpoint::Looking,
        };
        pane.rewind = Some(Rewind {
            before,
            checkout: checkout.clone(),
            checkpoint,
            draft,
            files: Files::Keep,
            focus: [(); 2].map(|()| cx.focus_handle().tab_stop(true)),
            lost_files,
            origin: origin.clone(),
            stage: Stage::Editing,
            workspace,
        });
        if let (Some(checkout), Some(origin)) = (checkout, origin) {
            self.checkpoints.find(checkout, origin);
        }
        input.update(cx, |input, cx| input.set_text_at(&typed, typed.len(), cx));
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.chrome_region = None;
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// Leaves the edit, putting back what the composer held. Once the files
    /// are going back or the program has the command, it is too late.
    pub(super) fn cancel_chat_rewind(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane
            .rewind
            .as_ref()
            .is_none_or(|rewind| rewind.stage != Stage::Editing)
        {
            return;
        }
        let Some(rewind) = pane.rewind.take() else {
            return;
        };
        let (text, attachments) = rewind.draft;
        pane.attachments = attachments;
        pane.error = None;
        if let Some(input) = pane.composer.clone() {
            input.update(cx, |input, cx| input.set_text_at(&text, text.len(), cx));
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
        self.chrome_region = None;
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// Sets what happens to the files, and gives typing back to the draft.
    pub(super) fn choose_chat_rewind_files(
        &mut self,
        pane_id: &str,
        files: Files,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let Some(rewind) = pane.rewind.as_mut().filter(|rewind| {
            rewind.stage == Stage::Editing
                && (files == Files::Keep
                    || matches!(
                        rewind.checkpoint,
                        Checkpoint::Looking | Checkpoint::Found(_)
                    ))
        }) else {
            return;
        };
        rewind.files = files;
        if let Some(input) = pane.composer.clone() {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
        self.chrome_region = None;
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// Tab from the draft: the keyboard moves to the Keep or Restore choice,
    /// and the region's keys take it from there.
    pub(super) fn focus_chat_rewind_choice(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rewind) = self
            .chats
            .pane(pane_id)
            .and_then(|pane| pane.rewind.as_ref())
        else {
            return;
        };
        let focus = rewind.focus[usize::from(rewind.files == Files::Restore)].clone();
        self.chrome_region = Some(Region::Chat);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Sends the edit, after its files when they go back too.
    pub(super) fn submit_chat_rewind(&mut self, pane_id: &str, text: String) {
        let restoring = self
            .chats
            .pane(pane_id)
            .and_then(|pane| pane.rewind.as_ref()?.checkout.as_ref())
            .is_some_and(|checkout| self.chat_restoring(pane_id, checkout));
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane.session.transcript().turn.is_some() {
            pane.error = Some(Rejection::TurnRunning.localize());
            return;
        }
        let attachments = attachments(pane);
        let Some(rewind) = pane.rewind.as_mut() else {
            return;
        };
        if rewind.stage != Stage::Editing {
            return;
        }
        if rewind.files == Files::Keep {
            send(pane, text, attachments);
            return;
        }
        let (Checkpoint::Found(id), Some(checkout)) = (&rewind.checkpoint, &rewind.checkout) else {
            pane.error = Some(fl!("chat-rewind-checkpoint-missing"));
            return;
        };
        if restoring {
            pane.error = Some(fl!("chat-rewind-restore-busy"));
            return;
        }
        match self.checkpoints.restore_id(checkout.clone(), id.clone()) {
            Ok(ticket) => {
                rewind.stage = Stage::Restoring {
                    attachments,
                    text,
                    ticket,
                };
                pane.error = None;
            }
            Err(error) => pane.error = Some(error.localize()),
        }
    }

    /// Whether a chat other than `pane_id` is putting back `checkout`'s files.
    /// Its agent starts as that restore ends, so a second one would change
    /// the files under it.
    pub(crate) fn chat_restoring(&self, pane_id: &str, checkout: &Checkout) -> bool {
        self.chats.panes().any(|pane| {
            pane.target.pane_id != pane_id
                && pane.rewind.as_ref().is_some_and(|rewind| {
                    matches!(rewind.stage, Stage::Restoring { .. })
                        && rewind.checkout.as_ref() == Some(checkout)
                })
        })
    }

    /// A checkpoint search finished: the edit it is for learns what it found.
    pub(crate) fn chat_rewind_found(&mut self, found: Found) {
        for pane in self.chats.panes_mut() {
            let Some(rewind) = pane.rewind.as_mut() else {
                continue;
            };
            if rewind.checkout.as_ref() != Some(&found.checkout)
                || rewind.origin.as_ref() != Some(&found.origin)
            {
                continue;
            }
            rewind.checkpoint = match &found.result {
                Ok(Some(checkpoint))
                    if checkpoint.branch.as_deref() == Some(found.checkout.branch.as_str()) =>
                {
                    Checkpoint::Found(checkpoint.id.clone())
                }
                Ok(_) => Checkpoint::Missing,
                Err(_) => Checkpoint::Unreachable,
            };
            if !matches!(rewind.checkpoint, Checkpoint::Found(_)) {
                rewind.files = Files::Keep;
            }
        }
    }

    /// The restore `ticket` finished: the edit waiting for it goes out, or
    /// stays in the composer with the conversation untouched.
    pub(crate) fn chat_rewind_restored(&mut self, ticket: u64, failure: Option<String>) {
        for pane in self.chats.panes_mut() {
            let Some(rewind) = pane.rewind.as_mut().filter(|rewind| {
                matches!(rewind.stage, Stage::Restoring { ticket: waiting, .. } if waiting == ticket)
            }) else {
                continue;
            };
            let stage = std::mem::replace(&mut rewind.stage, Stage::Editing);
            if let Stage::Restoring {
                attachments, text, ..
            } = stage
            {
                match failure {
                    None => send(pane, text, attachments),
                    Some(failure) => pane.error = Some(failure),
                }
            }
            return;
        }
    }

    /// Ends the edits the programs have answered: done, the draft is gone,
    /// and refused, it stays with the reason showing. An edit whose prompt
    /// has left the transcript has nothing to go back to.
    pub(crate) fn poll_chat_rewinds(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for pane in self.chats.panes_mut() {
            let answer = pane.session.take_rewind_answer();
            let Some(rewind) = pane.rewind.as_mut() else {
                continue;
            };
            match answer {
                Some(None) => {
                    pane.rewind = None;
                    pane.attachments.clear();
                    if let Some(input) = pane.composer.clone() {
                        input.update(cx, MultilineInput::clear);
                    }
                    changed = true;
                }
                Some(Some(_)) => {
                    rewind.stage = Stage::Editing;
                    changed = true;
                }
                None if rewind.stage == Stage::Editing
                    && pane.session.transcript().entry(rewind.before).is_none() =>
                {
                    pane.rewind = None;
                    changed = true;
                }
                None => {}
            }
        }
        if changed {
            self.redraw_terminal(cx);
            cx.notify();
        }
    }
}

impl ChatPane {
    /// Whether the edit has gone out, or its files are going back first. Its
    /// answer is on the way, so nothing may take its place.
    pub(super) fn rewind_submitted(&self) -> bool {
        self.rewind
            .as_ref()
            .is_some_and(|rewind| rewind.stage != Stage::Editing)
    }
}

fn attachments(pane: &ChatPane) -> Vec<Attachment> {
    pane.attachments
        .iter()
        .map(|path| Attachment {
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// Hands the edit to the program; an error leaves it in the composer.
fn send(pane: &mut ChatPane, text: String, attachments: Vec<Attachment>) {
    let Some(before) = pane.rewind.as_ref().map(|rewind| rewind.before) else {
        return;
    };
    match pane.send(Command::Rewind {
        attachments,
        before,
        text,
    }) {
        Ok(()) => {
            pane.error = None;
            if let Some(rewind) = pane.rewind.as_mut() {
                rewind.stage = Stage::Sending;
            }
        }
        Err(error) => pane.error = Some(error.localize()),
    }
}

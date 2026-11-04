//! Worktree notes: listed from the app's store, read and written for the
//! caller's checkout. Notes stay in this app; nothing reaches the daemon.
use super::{super::window_for, find};
use crate::{
    control::protocol::{
        Caller, ErrorCode, MAX_MESSAGE, Response,
        work::{NoteInfo, Reply},
    },
    worktree_notes::{Checkout, Note, Notes},
};
use gpui::App;
use sheprd_client::protocol::ClientShellWorkspace;

/// The checkout a workspace's note is about, once it is a Git checkout on a
/// branch, as the note dialog requires.
pub(super) fn checkout_of(endpoint: &str, workspace: &ClientShellWorkspace) -> Option<Checkout> {
    Some(Checkout {
        endpoint: endpoint.to_owned(),
        repo_key: workspace.worktree.as_ref()?.key.clone(),
        branch: workspace
            .branch
            .clone()
            .filter(|branch| !branch.is_empty())?,
    })
}

fn info(checkout: &Checkout, text: &str) -> NoteInfo {
    NoteInfo {
        host: checkout.endpoint.clone(),
        repository: checkout.repo_key.clone(),
        branch: checkout.branch.clone(),
        text: text.to_owned(),
    }
}

impl From<&Note> for NoteInfo {
    fn from(note: &Note) -> Self {
        info(&note.checkout, &note.text)
    }
}

/// Room for notes in one answer, short of the socket's message bound.
const LIST_BUDGET: usize = MAX_MESSAGE - 4096;

/// The most recent notes that fit one answer: up to 512 notes of 500
/// characters, with their keys, could pass the socket's bound.
pub(super) fn list(cx: &mut App) -> Response {
    let all: Vec<NoteInfo> = Notes::of(cx)
        .map(|notes| notes.recent().map(NoteInfo::from).collect())
        .unwrap_or_default();
    let (notes, omitted) = within(all, LIST_BUDGET);
    Response::Work {
        reply: Reply::Notes { notes, omitted },
    }
}

/// The leading notes whose JSON fits `budget`, and how many did not.
pub(super) fn within(all: Vec<NoteInfo>, budget: usize) -> (Vec<NoteInfo>, usize) {
    let total = all.len();
    let mut used = 0;
    let notes: Vec<_> = all
        .into_iter()
        .take_while(|note| {
            used += serde_json::to_vec(note).map_or(budget, |json| json.len() + 1);
            used <= budget
        })
        .collect();
    let omitted = total - notes.len();
    (notes, omitted)
}

/// The caller's checkout, as the window showing its workspace names it.
fn caller_checkout(caller: &Caller, cx: &mut App) -> Result<Checkout, Response> {
    let handle = window_for(caller, cx)?;
    handle
        .read_with(cx, |view, _| {
            let found = find(view, caller)?;
            checkout_of(&view.endpoints[found.index].id, found.workspace).ok_or_else(|| {
                Response::error(
                    ErrorCode::InvalidRequest,
                    "Notes are kept on Git checkouts with a branch",
                )
            })
        })
        .unwrap_or_else(|_| Err(Response::error(ErrorCode::NoWindow, "The window closed")))
}

pub(super) fn get(caller: &Caller, cx: &mut App) -> Response {
    let checkout = match caller_checkout(caller, cx) {
        Ok(checkout) => checkout,
        Err(response) => return response,
    };
    let note = Notes::of(cx)
        .and_then(|notes| notes.get(&checkout.endpoint, &checkout.repo_key, &checkout.branch))
        .map(|text| info(&checkout, text));
    Response::Work {
        reply: Reply::Note { note },
    }
}

pub(super) fn set(caller: &Caller, text: &str, cx: &mut App) -> Response {
    let checkout = match caller_checkout(caller, cx) {
        Ok(checkout) => checkout,
        Err(response) => return response,
    };
    let branch = checkout.branch.clone();
    let key = checkout.clone();
    Notes::update(cx, |notes| notes.set(checkout, text));
    let saved = Notes::of(cx)
        .and_then(|notes| notes.get(&key.endpoint, &key.repo_key, &key.branch))
        .is_some();
    match (saved, crate::worktree_notes::clean(text).is_empty()) {
        (true, _) => Response::done(format!("Saved the note on {branch}")),
        (false, true) => Response::done(format!("Cleared the note on {branch}")),
        (false, false) => Response::error(
            ErrorCode::InvalidRequest,
            "This checkout's repository or branch name is too long to keep a note on",
        ),
    }
}

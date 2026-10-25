//! `notes`, `checkpoint`, `scripts`, `teleport`, and `fan-out`.
mod checkpoint;
mod here;
mod scripts;
#[cfg(test)]
mod tests;

use super::{Context, Output, unexpected};
use crate::{
    cli::control::{FanOut, NotesCommand, Script},
    control::{
        ErrorCode,
        protocol::{
            Request, Response,
            work::{FanOutRequest, NoteInfo, Reply, WorkOp},
        },
    },
};
pub(super) use checkpoint::checkpoint;
use std::fmt::Write as _;

/// The display boundary for errors of the commands this process runs itself:
/// the message with its causes, and the exit status the refusal deserves.
fn rejected(error: impl Into<crate::Error>) -> crate::Error {
    let error = error.into();
    let code = match &error {
        crate::Error::Vcs(herdr_vcs::Error::Checkpoint(crate::checkpoint::Error::Missing)) => {
            ErrorCode::NotFound
        }
        crate::Error::NotInCheckout
        | crate::Error::DetachedHead
        | crate::Error::Vcs(herdr_vcs::Error::Checkpoint(
            crate::checkpoint::Error::Checkout | crate::checkpoint::Error::Unborn,
        )) => ErrorCode::InvalidRequest,
        _ => ErrorCode::Failed,
    };
    crate::control::Error::Rejected {
        code,
        message: crate::checkpoint::describe(&error),
    }
    .into()
}

/// A repository's name from its Git common directory: the directory the
/// `.git` is in, or a bare repository's own name.
pub(super) fn repository_name(repo_key: &str) -> &str {
    let key = repo_key.trim_end_matches('/');
    let key = key.strip_suffix("/.git").unwrap_or(key);
    key.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(repo_key)
}

pub(super) fn describe_note(note: &NoteInfo) -> String {
    format!(
        "{} ({}, {}): {}",
        note.branch,
        repository_name(&note.repository),
        note.host,
        note.text
    )
}

pub(super) fn notes(command: NotesCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        NotesCommand::List => WorkOp::Notes,
        NotesCommand::Get => WorkOp::Note,
        NotesCommand::Set(text) => WorkOp::SetNote { text },
        NotesCommand::Clear => WorkOp::SetNote {
            text: String::new(),
        },
    };
    match context.call(Request::Work, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::Work {
            reply: Reply::Notes { notes, omitted },
        } => {
            let json = serde_json::json!({ "notes": notes, "omitted": omitted });
            let mut text = String::new();
            for note in &notes {
                let _ = writeln!(text, "{}", describe_note(note));
            }
            if omitted > 0 {
                let _ = writeln!(text, "… and {omitted} older notes that did not fit");
            }
            if notes.is_empty() && omitted == 0 {
                return Ok(Output::data("No worktree notes", &json)?.nothing());
            }
            Output::data(text.trim_end(), &json)
        }
        Response::Work {
            reply: Reply::Note { note: Some(note) },
        } => Output::data(note.text.clone(), &serde_json::json!({ "note": note })),
        Response::Work {
            reply: Reply::Note { note: None },
        } => Ok(Output::data(
            "This checkout has no note",
            &serde_json::json!({ "note": null }),
        )?
        .nothing()),
        response => Err(unexpected(response)),
    }
}

pub(super) fn scripts(kind: Option<Script>, context: &Context) -> crate::Result<Output> {
    match kind {
        None => scripts::list(),
        Some(kind) => context.act(Request::Work, WorkOp::Script { kind }),
    }
}

pub(super) fn teleport(
    worktree: Option<&str>,
    host: String,
    context: &Context,
) -> crate::Result<Output> {
    if let Some(worktree) = worktree {
        return Err(crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            message: format!(
                "teleport moves the caller's own worktree, not {worktree}; run it from a pane there"
            ),
        }
        .into());
    }
    context.act(Request::Work, WorkOp::Teleport { host })
}

pub(super) fn fan_out(fan_out: FanOut, context: &Context) -> crate::Result<Output> {
    context.act(
        Request::Work,
        WorkOp::FanOut(FanOutRequest {
            prompt: fan_out.prompt,
            agents: fan_out.agents,
        }),
    )
}

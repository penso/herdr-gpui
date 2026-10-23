//! `tab`, `code`, `devices`, `orchestrator`, and `review`.
mod items;

use super::{Context, Output, unexpected};
use crate::{
    cli::control::{CodeCommand, FileAt, OrchestratorCommand, TabCommand},
    control::{
        ErrorCode,
        protocol::{
            Request, Response,
            tabs::{Reply, TabInfo, TabOp},
        },
    },
};
use std::{fmt::Write as _, path::Path};

fn rejected(code: ErrorCode, message: impl Into<String>) -> crate::Error {
    crate::control::Error::Rejected {
        code,
        message: message.into(),
    }
    .into()
}

/// One line per tab: its ID, kind, title, what it shows, and where.
fn describe(tabs: &[TabInfo]) -> String {
    let mut text = String::new();
    for tab in tabs {
        let _ = write!(text, "{}\t{}\t{}", tab.id, tab.kind.name(), tab.title);
        if let Some(location) = &tab.location {
            let _ = write!(text, "\t{location}");
        }
        let _ = write!(text, "\t(workspace {}", tab.workspace);
        if let Some(pane) = &tab.opened_by {
            let _ = write!(text, ", opened by {pane}");
        }
        let _ = writeln!(text, ")");
    }
    text.trim_end().to_owned()
}

pub(super) fn list(pages: bool, context: &Context) -> crate::Result<Output> {
    match context.call(Request::Tab, TabOp::List { pages })? {
        Response::Tab {
            reply: Reply::Tabs { tabs },
        } => {
            if tabs.is_empty() {
                let text = if pages {
                    "No page tabs are open"
                } else {
                    "No app tabs are open"
                };
                return Output::data(text, &tabs);
            }
            Output::data(describe(&tabs), &tabs)
        }
        response => Err(unexpected(response)),
    }
}

pub(super) fn tab(command: TabCommand, context: &Context) -> crate::Result<Output> {
    match command {
        TabCommand::List => list(false, context),
        TabCommand::Focus { id } => context.act(Request::Tab, TabOp::Focus { id }),
        TabCommand::Close { id } => context.act(Request::Tab, TabOp::Close { id }),
    }
}

/// The app reads the file itself, so it gets an absolute, resolved path to
/// a regular file, checked here where the caller's directory is known.
fn local_file(path: &Path) -> crate::Result<String> {
    let resolved = path.canonicalize().map_err(|error| {
        rejected(
            ErrorCode::NotFound,
            format!("Cannot open {}: {error}", path.display()),
        )
    })?;
    if !resolved.is_file() {
        return Err(rejected(
            ErrorCode::InvalidRequest,
            format!("{} is not a regular file", resolved.display()),
        ));
    }
    resolved
        .into_os_string()
        .into_string()
        .map_err(|_| rejected(ErrorCode::InvalidRequest, "The path must be UTF-8"))
}

/// The folder `--repo` names, resolved here for the app.
fn local_folder(path: &Path) -> crate::Result<String> {
    let resolved = path.canonicalize().map_err(|error| {
        rejected(
            ErrorCode::NotFound,
            format!("Cannot open {}: {error}", path.display()),
        )
    })?;
    if !resolved.is_dir() {
        return Err(rejected(
            ErrorCode::InvalidRequest,
            format!("{} is not a folder", resolved.display()),
        ));
    }
    resolved
        .into_os_string()
        .into_string()
        .map_err(|_| rejected(ErrorCode::InvalidRequest, "The path must be UTF-8"))
}

/// What `Opened` says, for a person.
fn opened(what: &str, response: Response) -> crate::Result<Output> {
    match response {
        Response::Tab { reply } => match &reply {
            Reply::Opened { id, workspace } => Output::data(
                format!("Opened {what} in tab {id} of workspace {workspace}"),
                &reply,
            ),
            _ => Err(unexpected(Response::Tab { reply })),
        },
        Response::Done { message } => Ok(Output::message(message)),
        response => Err(unexpected(response)),
    }
}

pub(super) fn code(command: CodeCommand, context: &Context) -> crate::Result<Output> {
    match command {
        CodeCommand::Open(FileAt { path, line }) => {
            let path = local_file(&path)?;
            let response = context.call(
                Request::Tab,
                TabOp::Code {
                    path: path.clone(),
                    line,
                },
            )?;
            opened(&path, response)
        }
        CodeCommand::Edit(FileAt { path, line }) => {
            let path = local_file(&path)?;
            context.act(Request::Tab, TabOp::Edit { path, line })
        }
    }
}

pub(super) fn devices(context: &Context) -> crate::Result<Output> {
    let response = context.call(Request::Tab, TabOp::Devices)?;
    opened("the Devices overview", response)
}

pub(super) fn px0(context: &Context) -> crate::Result<Output> {
    opened("px0", context.call(Request::Tab, TabOp::Px0)?)
}

pub(super) fn orchestrator(
    command: OrchestratorCommand,
    context: &Context,
) -> crate::Result<Output> {
    match command {
        OrchestratorCommand::Open { repo } => {
            let repo = repo.as_deref().map(local_folder).transpose()?;
            let response = context.call(Request::Tab, TabOp::Orchestrator { repo })?;
            opened("the orchestrator", response)
        }
        OrchestratorCommand::Items { repo, items } => items::list(repo.as_deref(), items),
    }
}

pub(super) fn review(context: &Context) -> crate::Result<Output> {
    let response = context.call(Request::Tab, TabOp::Review)?;
    opened("the review", response)
}

#[cfg(test)]
mod tests;

//! `browser ...` and `feedback`.
use super::{Context, Output, caller, notes_daemon, rejected, send, tabs, unexpected};
use crate::{
    browser::{LocalFile, WebUrl},
    cli::control::{BrowserCommand, Placement, ZoomChange},
    control::protocol::{
        BrowserOpen, Caller, ErrorCode, FeedbackRequest, MAX_WAIT_SECONDS, OpenedIn, Page, Request,
        Response,
        tabs::{Reply, TabOp},
    },
};
use std::path::Path;

/// The explainer kit's example, which the agent skill points to.
pub(super) const EXAMPLE: &str =
    include_str!("../../../../../skills/herdr-gpui-browser/examples/instagram-fetch.html");

/// A file that exists is shown as a local page; anything else is an address.
fn page(target: &str) -> crate::Result<(Page, String)> {
    let path = Path::new(target);
    if !target.contains("://") && path.is_file() {
        let path = path.canonicalize()?;
        let home = crate::config::home().ok();
        LocalFile::new(&path, home.as_deref())?;
        let text = path
            .to_str()
            .ok_or(crate::Error::InvalidLocalPage)?
            .to_owned();
        return Ok((Page::File(text.clone()), text));
    }
    let url = WebUrl::from_typed(target)?;
    Ok((Page::Url(url.as_str().to_owned()), url.as_str().to_owned()))
}

/// Where a page goes unless the caller said: a local file, usually a mockup
/// or an explainer the agent wrote, sits beside the terminal that opened it,
/// so the user reads it and the agent's replies together. Web addresses, and
/// files opened from outside a Herdr pane, take a tab.
pub(super) fn default_placement(page: &Page, caller: &Caller) -> Placement {
    match (page, &caller.pane_id) {
        (Page::File(_), Some(_)) => Placement::Beside,
        _ => Placement::Tab,
    }
}

fn open(
    target: &str,
    workspace: Option<String>,
    focus: bool,
    placement: Option<Placement>,
    context: &Context,
) -> crate::Result<Output> {
    let (page, shown) = page(target)?;
    let caller = Caller {
        window: context.window,
        ..caller(workspace)?
    };
    let placement = placement.unwrap_or_else(|| default_placement(&page, &caller));
    let request = Request::Open(BrowserOpen {
        page,
        caller,
        focus,
        placement,
    });
    let response = send(&request)?;
    let text = match &response {
        Response::Opened {
            opened_in: OpenedIn::Tab,
            workspace_id,
            placement: opened,
        } => {
            let place = match opened {
                Placement::Beside => "beside the terminal",
                Placement::Tab => "in a browser tab",
            };
            let fallback = if placement == Placement::Beside && *opened == Placement::Tab {
                " (your terminal is not on screen, so the page could not go beside it)"
            } else {
                ""
            };
            match workspace_id {
                Some(id) => format!("Opened {shown} {place} of workspace {id}{fallback}"),
                None => format!("Opened {shown} {place}{fallback}"),
            }
        }
        Response::Opened {
            opened_in: OpenedIn::SystemBrowser,
            ..
        } => format!(
            "Opened {shown} in the system browser; this platform's build has no browser tabs"
        ),
        _ => return Err(rejected(response)),
    };
    Output::data(text, &response)
}

fn reload() -> crate::Result<Output> {
    match send(&Request::Reload(caller(None)?))? {
        Response::Reloaded { tabs: 0 } => Err(crate::Error::ControlRejected {
            code: ErrorCode::InvalidRequest,
            message: "This pane has no browser tabs open".into(),
        }),
        Response::Reloaded { tabs } => Output::data(
            format!("Reloaded {tabs} browser tab(s)"),
            &serde_json::json!({ "tabs": tabs }),
        ),
        response => Err(rejected(response)),
    }
}

/// The user's notes for this pane: page, review, and terminal notes alike
/// are kept for it whenever they could not be typed into the pane.
pub(super) fn feedback(wait: u64, _: &Context) -> crate::Result<Output> {
    let pane_id = super::env_id("HERDR_PANE_ID").ok_or(crate::Error::ControlRejected {
        code: ErrorCode::InvalidRequest,
        message: "feedback runs in the Herdr pane that should receive the notes".into(),
    })?;
    let request = Request::Feedback(FeedbackRequest {
        pane_id,
        daemon_socket: Some(notes_daemon()?),
        wait_seconds: wait.min(MAX_WAIT_SECONDS),
    });
    match send(&request)? {
        // Notes keep their line breaks: they are Markdown for the agent.
        Response::Feedback { text: Some(text) } => {
            Output::data(text.trim_end(), &serde_json::json!({ "notes": text }))
        }
        Response::Feedback { text: None } => Ok(Output::data(
            "No notes from the user yet",
            &serde_json::json!({ "notes": null }),
        )?
        .nothing()),
        response => Err(rejected(response)),
    }
}

/// The text of a page tab.
fn text(id: u64, context: &Context) -> crate::Result<Output> {
    match context.call(Request::Tab, TabOp::Text { id })? {
        Response::Tab {
            reply: Reply::Text { text },
        } => Output::data(text.clone(), &serde_json::json!({ "id": id, "text": text })),
        response => Err(unexpected(response)),
    }
}

/// Zooms a page tab.
fn zoom(id: u64, change: ZoomChange, context: &Context) -> crate::Result<Output> {
    zoomed(context.call(Request::Tab, TabOp::Zoom { id, change })?)
}

/// What a zoomed tab says: the zoom it landed on.
fn zoomed(response: Response) -> crate::Result<Output> {
    match response {
        Response::Tab {
            reply: Reply::Zoom { id, percent },
        } => Output::data(
            format!("Tab {id} is zoomed to {percent}%"),
            &serde_json::json!({ "id": id, "zoom": percent }),
        ),
        response => Err(unexpected(response)),
    }
}

/// The screenshot is written by the app, so the path is made absolute here,
/// where the caller's working directory is known.
fn screenshot(id: u64, path: &Path, context: &Context) -> crate::Result<Output> {
    let path = std::path::absolute(path)?;
    let text = path
        .to_str()
        .ok_or(crate::Error::InvalidLocalPage)?
        .to_owned();
    context.act(Request::Tab, TabOp::Screenshot { id, path: text })
}

pub(super) fn run(command: BrowserCommand, context: &Context) -> crate::Result<Output> {
    match command {
        // Answered before any request; see `client::run`.
        BrowserCommand::Help | BrowserCommand::Skill | BrowserCommand::Example => {
            Ok(Output::message(""))
        }
        BrowserCommand::Open {
            target,
            workspace,
            focus,
            placement,
        } => open(&target, workspace, focus, placement, context),
        BrowserCommand::Reload => reload(),
        BrowserCommand::Feedback { wait } => feedback(wait, context),
        BrowserCommand::List => tabs::list(true, context),
        BrowserCommand::Close { id } => context.act(Request::Tab, TabOp::Close { id }),
        BrowserCommand::Navigate { id, url } => {
            let url = WebUrl::from_typed(&url)?;
            context.act(
                Request::Tab,
                TabOp::Navigate {
                    id,
                    url: url.as_str().to_owned(),
                },
            )
        }
        BrowserCommand::Back { id } => context.act(Request::Tab, TabOp::Back { id }),
        BrowserCommand::Forward { id } => context.act(Request::Tab, TabOp::Forward { id }),
        BrowserCommand::Text { id } => text(id, context),
        BrowserCommand::Screenshot { id, path } => screenshot(id, &path, context),
        BrowserCommand::Zoom { id, change } => zoom(id, change, context),
    }
}

#[cfg(test)]
mod tests;

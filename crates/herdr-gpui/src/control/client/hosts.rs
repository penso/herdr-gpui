//! `pr`, `host`, `cloud`, `ports`, `usage`, `system`, `security`, and
//! `automation`.
mod batch;
mod cloud;
mod exec;
mod text;

pub(super) use text::bytes;

#[cfg(test)]
mod tests;

use super::{Context, Output, unexpected};
use crate::{
    cli::control::{
        AutomationCommand, CloudCommand, HostCommand, PortsCommand, PrCommand, SecurityCommand,
    },
    control::protocol::{
        ErrorCode, Request, Response,
        hosts::{HostOp, Reply},
    },
};

/// No pull request: said on stderr, exiting [`super::exit::NOTHING`].
fn no_pr(note: Option<String>) -> crate::Result<Output> {
    let text = note.unwrap_or_else(|| "No pull request for this branch".into());
    Ok(Output::data(
        text.clone(),
        &serde_json::json!({ "pr": null, "note": text }),
    )?
    .nothing())
}

pub(super) fn pr(command: PrCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        PrCommand::Status | PrCommand::Checks => HostOp::Pr,
        PrCommand::Comments => HostOp::PrComments,
        PrCommand::Open => HostOp::PrOpen,
    };
    match context.call(Request::Host, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::Host {
            reply: Reply::Pr { pr: None, note },
        } => no_pr(note),
        Response::Host {
            reply: Reply::Pr { pr: Some(pr), .. },
        } => match command {
            PrCommand::Checks => Output::data(text::checks(&pr.checks), &pr.checks),
            _ => Output::data(text::pr(&pr), &pr),
        },
        Response::Host {
            reply: Reply::Comments { comments },
        } => {
            let output = Output::data(text::comments(&comments), &comments)?;
            Ok(if comments.is_empty() {
                output.nothing()
            } else {
                output
            })
        }
        response => Err(unexpected(response)),
    }
}

pub(super) fn host(command: HostCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        HostCommand::List => HostOp::List,
        HostCommand::Select(id) => HostOp::Select { id },
    };
    match context.call(Request::Host, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::Host {
            reply: Reply::Hosts { hosts },
        } => Output::data(text::hosts(&hosts), &hosts),
        response => Err(unexpected(response)),
    }
}

pub(super) fn cloud(command: CloudCommand, context: &Context) -> crate::Result<Output> {
    match command {
        CloudCommand::List => cloud::list(),
        CloudCommand::Add => context.act(Request::Host, HostOp::Cloud),
        CloudCommand::Setup(id) => context.act(Request::Host, HostOp::CloudSetup { id }),
        CloudCommand::Images => cloud::images(),
        CloudCommand::ImagesBuild(provider) => {
            context.act(Request::Host, HostOp::CloudImages { provider })
        }
        CloudCommand::Batch(command) => batch::run(command, context),
        CloudCommand::Exec(command) => exec::run(command, context),
    }
}

pub(super) fn ports(command: PortsCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        PortsCommand::List => HostOp::Ports,
        PortsCommand::Forward(port) => HostOp::Forward { port },
        PortsCommand::Unforward(port) => HostOp::Unforward { port },
        PortsCommand::Open(port) => HostOp::OpenPort { port },
    };
    match context.call(Request::Host, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::Host {
            reply: Reply::Ports(ports),
        } => {
            let empty = ports.listening.is_empty() && ports.forwards.is_empty();
            let output = Output::data(text::ports(&ports), &ports)?;
            Ok(if empty { output.nothing() } else { output })
        }
        response => Err(unexpected(response)),
    }
}

pub(super) fn usage(context: &Context) -> crate::Result<Output> {
    match context.call(Request::Host, HostOp::Usage)? {
        Response::Host {
            reply: reply @ Reply::Usage { .. },
        } => {
            let empty = matches!(&reply, Reply::Usage { usage, .. } if usage.is_empty());
            let output = Output::data(text::usage(&reply), &reply)?;
            Ok(if empty { output.nothing() } else { output })
        }
        response => Err(unexpected(response)),
    }
}

pub(super) fn security(command: SecurityCommand, context: &Context) -> crate::Result<Output> {
    if let SecurityCommand::Open { repository, item } = command {
        return context.act(Request::Host, HostOp::SecurityOpen { repository, item });
    }
    match context.call(Request::Host, HostOp::Security)? {
        Response::Host {
            reply: reply @ Reply::Security { .. },
        } => {
            let empty = matches!(&reply, Reply::Security { repositories, .. }
                if repositories.iter().all(|repo| repo.alerts.is_empty() && repo.reports.is_empty()));
            let output = Output::data(text::security(&reply), &reply)?;
            Ok(if empty { output.nothing() } else { output })
        }
        response => Err(unexpected(response)),
    }
}

pub(super) fn automation(command: AutomationCommand, context: &Context) -> crate::Result<Output> {
    let AutomationCommand::Approve { file, host } = command;
    let path = script_path(&file)?;
    context.act(Request::Host, HostOp::AutomationApprove { path, host })
}

/// The script's path as its host lists it: resolved where it exists here,
/// else made absolute against the caller's directory, since a remote
/// host's file is not on this machine.
pub(super) fn script_path(file: &std::path::Path) -> crate::Result<String> {
    let rejected = |message: String| {
        crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            message,
        }
        .into()
    };
    let path = match file.canonicalize() {
        Ok(path) => path,
        // Not here: a remote host's file. A Unix absolute path stays as it
        // is, so a Windows client does not put its own drive in front.
        Err(_) if file.to_str().is_some_and(|text| text.starts_with('/')) => file.to_owned(),
        Err(_) => std::path::absolute(file)
            .map_err(|error| rejected(format!("Cannot resolve {}: {error}", file.display())))?,
    };
    path.into_os_string()
        .into_string()
        .map_err(|_| rejected("The path must be UTF-8".to_owned()))
}

pub(super) fn system(context: &Context) -> crate::Result<Output> {
    match context.call(Request::Host, HostOp::System)? {
        Response::Host {
            reply: reply @ Reply::System { .. },
        } => {
            let empty = matches!(&reply, Reply::System { load: None, .. });
            let output = Output::data(text::system(&reply), &reply)?;
            Ok(if empty { output.nothing() } else { output })
        }
        response => Err(unexpected(response)),
    }
}

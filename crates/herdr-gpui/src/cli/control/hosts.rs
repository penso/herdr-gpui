//! `pr`, `host`, `cloud`, `ports`, `security`, and `automation`.
use super::{Args, BatchCommand, CliError, ExecCommand, batch, exec, unknown};
use herdr_control_protocol::protocol::hosts::PortAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCommand {
    Status,
    Checks,
    Comments,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCommand {
    List,
    Select(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudCommand {
    /// Saved cloud machines, read from this machine's settings.
    List,
    /// Open the settings that add one; creating a machine costs money, so
    /// the user chooses there.
    Add,
    /// Run the configured setup script on a saved machine again.
    Setup(String),
    /// Sandbox images, read from this machine's records.
    Images,
    /// Build the caller's repository's sandbox images, for one provider or
    /// every one that holds images.
    ImagesBuild(Option<String>),
    /// Machines handled in batches: destroying them, and following that.
    Batch(BatchCommand),
    /// One command on a saved machine.
    Exec(ExecCommand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecurityCommand {
    List,
    /// Show an alert (`42`) or a private report (`GHSA-…`) of `repository`
    /// in the section, beside its list.
    Open {
        repository: String,
        item: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutomationCommand {
    /// Open the approval of the script at `file` on `host` (an endpoint ID
    /// or label; absent, the caller's host). The user approves in the app.
    Approve {
        file: std::path::PathBuf,
        host: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortsCommand {
    List,
    Forward(u16),
    Unforward(u16),
    Open(u16),
    /// Every port this computer's user listens on.
    All,
    /// Stop or pause (through the app's confirmation) or resume the
    /// processes listening on a port of this computer.
    Signal(u16, PortAction),
}

pub(super) fn pr(args: &mut Args) -> Result<PrCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(PrCommand::Status);
    };
    Ok(match verb.to_str() {
        Some("status") => PrCommand::Status,
        Some("checks") => PrCommand::Checks,
        Some("comments") => PrCommand::Comments,
        Some("open") => PrCommand::Open,
        _ => return Err(unknown("pr", verb)),
    })
}

pub(super) fn host(args: &mut Args) -> Result<HostCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(HostCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => HostCommand::List,
        Some("select") => HostCommand::Select(args.required_text("HOST")?),
        _ => return Err(unknown("host", verb)),
    })
}

pub(super) fn security(args: &mut Args) -> Result<SecurityCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(SecurityCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => SecurityCommand::List,
        Some("open") => SecurityCommand::Open {
            repository: args.required_text("REPOSITORY")?,
            item: args.required_text("ALERT|GHSA")?,
        },
        _ => return Err(unknown("security", verb)),
    })
}

pub(super) fn automation(args: &mut Args) -> Result<AutomationCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "automation",
        what: "approve",
    })?;
    Ok(match verb.to_str() {
        Some("approve") => AutomationCommand::Approve {
            host: args.text_option("--host")?,
            file: args.required("FILE")?.into(),
        },
        _ => return Err(unknown("automation", verb)),
    })
}

pub(super) fn cloud(args: &mut Args) -> Result<CloudCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(CloudCommand::List);
    };
    if let Some(Some(command)) = verb
        .to_str()
        .map(|verb| batch::parse(verb, args))
        .transpose()?
    {
        return Ok(CloudCommand::Batch(command));
    }
    Ok(match verb.to_str() {
        Some("list") => CloudCommand::List,
        Some("add") => CloudCommand::Add,
        Some("setup") => CloudCommand::Setup(args.required_text("ID")?),
        Some("images") => match args.verb() {
            None => CloudCommand::Images,
            Some(verb) => match verb.to_str() {
                Some("list") => CloudCommand::Images,
                Some("build") => CloudCommand::ImagesBuild(args.optional_text()?),
                _ => return Err(unknown("cloud images", verb)),
            },
        },
        Some("exec") => CloudCommand::Exec(exec::parse(args)?),
        _ => return Err(unknown("cloud", verb)),
    })
}

fn port(args: &mut Args) -> Result<u16, CliError> {
    args.required_text("PORT")?
        .parse()
        .ok()
        .filter(|port| *port > 0)
        .ok_or(CliError::InvalidValue("PORT"))
}

pub(super) fn ports(args: &mut Args) -> Result<PortsCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(PortsCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => PortsCommand::List,
        Some("forward") => PortsCommand::Forward(port(args)?),
        Some("unforward") => PortsCommand::Unforward(port(args)?),
        Some("open") => PortsCommand::Open(port(args)?),
        Some("all") => PortsCommand::All,
        Some("stop") => PortsCommand::Signal(port(args)?, PortAction::Stop),
        Some("pause") => PortsCommand::Signal(port(args)?, PortAction::Pause),
        Some("resume") => PortsCommand::Signal(port(args)?, PortAction::Resume),
        _ => return Err(unknown("ports", verb)),
    })
}

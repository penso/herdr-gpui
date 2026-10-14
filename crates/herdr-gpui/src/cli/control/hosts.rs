//! `pr`, `host`, `cloud`, and `ports`.
use super::{Args, CliError, unknown};

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
    /// Open the saved SSH device's command editor; the user saves there.
    SshCommand(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudCommand {
    /// Saved cloud machines, read from this machine's settings.
    List,
    /// Open the settings that add one; creating a machine costs money, so
    /// the user chooses there.
    Add,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortsCommand {
    List,
    Forward(u16),
    Unforward(u16),
    Open(u16),
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
        Some("ssh-command") => HostCommand::SshCommand(args.required_text("HOST")?),
        _ => return Err(unknown("host", verb)),
    })
}

pub(super) fn cloud(args: &mut Args) -> Result<CloudCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(CloudCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => CloudCommand::List,
        Some("add") => CloudCommand::Add,
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
        _ => return Err(unknown("ports", verb)),
    })
}

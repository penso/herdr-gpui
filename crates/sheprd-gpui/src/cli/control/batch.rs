//! `cloud create`, `cloud destroy`, `cloud wait`, and `cloud batches`: cloud
//! machines handled in batches, which an agent follows until each machine is
//! settled.
use super::{Args, CliError, MAX_WAIT};
use crate::{
    cli::args::utf8,
    control::batch::{CreateOp, MAX_CREATE, MAX_DEVICES, Shape},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchCommand {
    /// Ask in the window to create machines.
    Create(CreateOp),
    /// Ask in the window to destroy saved devices: the ones named, and the
    /// ones a batch holds.
    Destroy {
        devices: Vec<String>,
        batch: Option<u64>,
    },
    /// A batch, once every device in it is settled or `timeout` passes.
    Wait { batch: u64, timeout: u64 },
    /// The batches the app keeps.
    List,
}

/// The command `verb` names, or `None` when it is not one of these.
pub(super) fn parse(verb: &str, args: &mut Args) -> Result<Option<BatchCommand>, CliError> {
    Ok(Some(match verb {
        "create" => BatchCommand::Create(CreateOp {
            count: args.number("--count", 1..=MAX_CREATE)?.unwrap_or(1),
            template: args.text_option("--template")?,
            preset: args.text_option("--preset")?,
            shape: if args.flag("--sandbox") {
                Shape::Sandbox
            } else {
                Shape::Machine
            },
            install: !args.flag("--no-install"),
            setup: !args.flag("--no-setup"),
            provider: args.required_text("PROVIDER")?,
        }),
        "destroy" => {
            let batch = args.number("--batch", 1..=u64::MAX)?;
            let mut devices = Vec::new();
            while let Some(device) = args.positional() {
                devices.push(utf8(device)?);
            }
            if devices.is_empty() && batch.is_none() {
                return Err(CliError::MissingArgument {
                    command: "cloud destroy",
                    what: "ID or --batch BATCH",
                });
            }
            if devices.len() > MAX_DEVICES {
                return Err(CliError::InvalidValue("ID"));
            }
            BatchCommand::Destroy { devices, batch }
        }
        "wait" => BatchCommand::Wait {
            timeout: args.number("--timeout", 1..=MAX_WAIT)?.unwrap_or(60),
            batch: args
                .required_text("BATCH")?
                .parse()
                .map_err(|_| CliError::InvalidValue("BATCH"))?,
        },
        "batches" => BatchCommand::List,
        _ => return Ok(None),
    }))
}

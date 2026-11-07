//! `cloud create`, `cloud destroy`, `cloud wait`, and `cloud batches`,
//! answered by the app.
//! `wait` exits 0 once every device of the batch went as asked, 1 once the
//! batch settled otherwise (a device failed, or the user said no), and 4
//! while it is still going.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::BatchCommand,
    control::protocol::{
        Request, Response,
        cloud::{BatchInfo, BatchKind, CloudOp, DeviceState, Phase, Reply},
    },
};

pub(super) fn run(command: BatchCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        BatchCommand::Create(create) => CloudOp::Create(create),
        BatchCommand::Destroy { devices, batch } => CloudOp::Destroy { devices, batch },
        BatchCommand::Wait { batch, timeout } => CloudOp::Wait {
            batch,
            wait_seconds: timeout,
        },
        BatchCommand::List => CloudOp::Batches,
    };
    let asked = matches!(op, CloudOp::Create(_) | CloudOp::Destroy { .. });
    match context.call(Request::Cloud, op)? {
        Response::Cloud {
            reply: Reply::Batch { batch },
        } if asked => Output::data(asked_text(&batch), &batch),
        Response::Cloud {
            reply: Reply::Batch { batch },
        } => {
            let output = Output::data(text(&batch), &batch)?;
            Ok(if !batch.phase.settled() {
                output.nothing()
            } else if batch.succeeded() {
                output
            } else {
                output.failed()
            })
        }
        Response::Cloud {
            reply: Reply::Batches { batches, omitted },
        } => {
            let mut text = if batches.is_empty() {
                "Sheprd keeps no cloud batch".to_owned()
            } else {
                batches.iter().map(text).collect::<Vec<_>>().join("\n\n")
            };
            if omitted > 0 {
                text.push_str(&format!(
                    "\n\n{omitted} older batch{} left out; follow one with `sheprd cloud wait BATCH`",
                    if omitted == 1 { " is" } else { "es are" }
                ));
            }
            let output = Output::data(text, &batches)?;
            Ok(if batches.is_empty() {
                output.nothing()
            } else {
                output
            })
        }
        response => Err(unexpected(response)),
    }
}

fn asked_text(batch: &BatchInfo) -> String {
    let count = batch.devices.len();
    let verb = match batch.kind {
        BatchKind::Create => "create",
        BatchKind::Destroy => "destroy",
    };
    format!(
        "Asked in Sheprd to {verb} {count} device{} as batch {id}; the user confirms there.\n\
         Follow it with `sheprd cloud wait {id}`.",
        if count == 1 { "" } else { "s" },
        id = batch.id,
    )
}

fn phase(phase: Phase) -> &'static str {
    match phase {
        Phase::Confirming => "waiting for the user to confirm",
        Phase::Running => "running",
        Phase::Done => "done",
        Phase::Cancelled => "cancelled",
    }
}

fn state(state: DeviceState) -> &'static str {
    match state {
        DeviceState::Waiting => "waiting",
        DeviceState::Creating => "creating",
        DeviceState::SettingUp => "running the setup script",
        DeviceState::Ready => "ready",
        DeviceState::Destroying => "destroying",
        DeviceState::Destroyed => "destroyed",
        DeviceState::Forgotten => "removed from Sheprd; its provider cannot delete it from here",
        DeviceState::Failed => "failed",
        DeviceState::Cancelled => "cancelled",
    }
}

pub(super) fn text(batch: &BatchInfo) -> String {
    let mut lines = vec![format!("Batch {}: {}", batch.id, phase(batch.phase))];
    lines.extend(batch.devices.iter().map(|device| {
        let mut line = if device.endpoint.is_empty() {
            format!("{}: {}", device.label, state(device.state))
        } else {
            format!(
                "{} ({}): {}",
                device.label,
                device.endpoint,
                state(device.state)
            )
        };
        if let Some(step) = &device.step {
            line.push_str(", ");
            line.push_str(step);
        }
        if let Some(error) = &device.error {
            line.push_str(": ");
            line.push_str(error);
        }
        line
    }));
    lines.join("\n")
}

#[cfg(test)]
mod tests;

//! `cloud exec`: one command on a saved cloud machine, run by the app. Its
//! stdout goes to stdout (byte for byte into a pipe) and its stderr to
//! stderr; it exits 0 when the command did, and 1 otherwise.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::ExecCommand,
    control::protocol::{
        Request, Response,
        cloud::{CloudOp, ExecInfo, Reply},
    },
};

pub(super) fn run(command: ExecCommand, context: &Context) -> crate::Result<Output> {
    let op = CloudOp::Exec {
        device: command.device,
        command: command.command,
        cwd: command.cwd,
        wait_seconds: command.timeout,
    };
    match context.call(Request::Cloud, op)? {
        Response::Cloud {
            reply: Reply::Exec(info),
        } => output(&info),
        response => Err(unexpected(response)),
    }
}

fn output(info: &ExecInfo) -> crate::Result<Output> {
    let mut stderr = info.stderr.clone();
    if info.truncated {
        stderr.push_str("[sheprd: only the end of the output was kept]\n");
    }
    if info.status != Some(0) {
        stderr.push_str(&match info.status {
            Some(code) => format!("[sheprd: the command exited {code}]\n"),
            None => "[sheprd: a signal ended the command]\n".to_owned(),
        });
    }
    let output = Output::bytes(info.stdout.clone().into_bytes(), info)?.with_stderr(stderr);
    Ok(if info.status == Some(0) {
        output
    } else {
        output.failed()
    })
}

#[cfg(test)]
mod tests;

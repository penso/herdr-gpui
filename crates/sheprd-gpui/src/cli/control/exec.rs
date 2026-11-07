//! `cloud exec ID [--timeout S] [--cwd DIR] -- COMMAND...`: one command on a
//! saved cloud machine. Everything after `--` is the command, word for word.
use super::{Args, CliError, MAX_WAIT};
use crate::cli::args::utf8;

/// How long a command may take when `--timeout` does not say.
const DEFAULT_TIMEOUT: u64 = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecCommand {
    /// The saved device: endpoint ID, machine ID, or label.
    pub device: String,
    /// The program and its arguments.
    pub command: Vec<String>,
    /// Where it runs; `~/` is the machine's home directory.
    pub cwd: Option<String>,
    /// Seconds in all, including waiting for the machine to start.
    pub timeout: u64,
}

pub(super) fn parse(args: &mut Args) -> Result<ExecCommand, CliError> {
    let missing = || CliError::MissingArgument {
        command: "cloud exec",
        what: "-- COMMAND",
    };
    let command = args
        .trailing()
        .ok_or_else(missing)?
        .into_iter()
        .map(utf8)
        .collect::<Result<Vec<_>, _>>()?;
    if command.first().is_none_or(String::is_empty) {
        return Err(missing());
    }
    Ok(ExecCommand {
        timeout: args
            .number("--timeout", 1..=MAX_WAIT)?
            .unwrap_or(DEFAULT_TIMEOUT),
        cwd: args.text_option("--cwd")?,
        device: args.required_text("ID")?,
        command,
    })
}

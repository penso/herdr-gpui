//! `notes`, `checkpoint`, `scripts`, `fan-out`, and `wait`.
use super::{Args, CliError, Script, unknown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotesCommand {
    List,
    Get,
    Set(String),
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointCommand {
    List,
    Create {
        label: Option<String>,
    },
    /// What changed from checkpoint `from` to `to`, or to the working tree.
    Diff {
        from: String,
        to: Option<String>,
    },
    /// Open the checkpoints dialog with `id` chosen; the user confirms.
    Restore {
        id: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanOut {
    pub prompt: String,
    /// `--agent claude:2`: kind names, each with how many.
    pub agents: Vec<(String, u8)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitFor {
    Feedback,
    Window,
    TabClosed(u64),
    Online,
}

/// Longest text accepted for a note, a label, or a prompt, before the app
/// applies its own limits.
const MAX_TEXT: usize = 16 * 1024;

fn bounded(text: String) -> Result<String, CliError> {
    if text.len() > MAX_TEXT {
        return Err(CliError::InvalidValue("text over 16 KiB"));
    }
    Ok(text)
}

pub(super) fn notes(args: &mut Args) -> Result<NotesCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(NotesCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => NotesCommand::List,
        Some("get") => NotesCommand::Get,
        Some("set") => NotesCommand::Set(bounded(args.words()?.ok_or(
            CliError::MissingArgument {
                command: "notes set",
                what: "TEXT",
            },
        )?)?),
        Some("clear") => NotesCommand::Clear,
        _ => return Err(unknown("notes", verb)),
    })
}

/// A checkpoint ID as `checkpoint list` prints it: digits, as the app's
/// own check accepts them.
fn checkpoint_id(text: String) -> Result<String, CliError> {
    let valid = !text.is_empty() && text.len() <= 32 && text.bytes().all(|b| b.is_ascii_digit());
    valid
        .then_some(text)
        .ok_or(CliError::InvalidValue("CHECKPOINT"))
}

pub(super) fn checkpoint(args: &mut Args) -> Result<CheckpointCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(CheckpointCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => CheckpointCommand::List,
        Some("create") => CheckpointCommand::Create {
            label: args.words()?.map(bounded).transpose()?,
        },
        Some("diff") => CheckpointCommand::Diff {
            from: checkpoint_id(args.required_text("CHECKPOINT")?)?,
            to: args.optional_text()?.map(checkpoint_id).transpose()?,
        },
        Some("restore") => CheckpointCommand::Restore {
            id: args.optional_text()?.map(checkpoint_id).transpose()?,
        },
        _ => return Err(unknown("checkpoint", verb)),
    })
}

pub(super) fn script_kind(args: &mut Args) -> Result<Option<Script>, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(None);
    };
    Ok(match verb.to_str() {
        Some("list") => None,
        Some("setup") => Some(Script::Setup),
        Some("run") => Some(Script::Run),
        Some("archive") => Some(Script::Archive),
        _ => return Err(unknown("scripts", verb)),
    })
}

pub(super) fn fan_out(args: &mut Args) -> Result<FanOut, CliError> {
    let agents = args
        .options("--agent")?
        .into_iter()
        .map(|agent| {
            let text = agent
                .to_str()
                .ok_or(CliError::InvalidValue("--agent KIND[:COUNT]"))?;
            let (kind, count) = text.split_once(':').unwrap_or((text, "1"));
            let count = count
                .parse::<u8>()
                .ok()
                .filter(|count| (1..=8).contains(count));
            match count {
                Some(count) if !kind.is_empty() => Ok((kind.to_ascii_lowercase(), count)),
                _ => Err(CliError::InvalidValue("--agent KIND[:COUNT]")),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let prompt = bounded(args.words()?.ok_or(CliError::MissingArgument {
        command: "fan-out",
        what: "PROMPT",
    })?)?;
    Ok(FanOut { prompt, agents })
}

pub(super) fn wait_for(args: &mut Args) -> Result<WaitFor, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "wait",
        what: "feedback|window|tab-closed|online",
    })?;
    Ok(match verb.to_str() {
        Some("feedback") => WaitFor::Feedback,
        Some("window") => WaitFor::Window,
        Some("online") => WaitFor::Online,
        Some("tab-closed") => WaitFor::TabClosed(
            args.required_text("ID")?
                .parse()
                .map_err(|_| CliError::InvalidValue("ID"))?,
        ),
        _ => return Err(unknown("wait", verb)),
    })
}

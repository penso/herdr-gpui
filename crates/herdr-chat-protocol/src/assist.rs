//! What helps the user write a prompt, the same in every client: completing
//! the agent's slash commands, recalling earlier prompts, and stepping the
//! session's settings (model, thinking level, mode).

use crate::{
    ATTACHED, Command, ConfigOption, EntryKind, OptionCategory, OptionKind, OptionValue,
    SlashCommand, Transcript,
};

/// Commands offered for completion at most.
pub const COMPLETIONS: usize = 6;
/// Prompts remembered for recall.
const HISTORY: usize = 100;

/// The agent's commands that complete `text`, a `/` and part of a name with
/// nothing after it yet.
pub fn completions<'a>(transcript: &'a Transcript, text: &str) -> Vec<&'a SlashCommand> {
    let Some(typed) = text.strip_prefix('/') else {
        return Vec::new();
    };
    if typed.contains(char::is_whitespace) {
        return Vec::new();
    }
    transcript
        .commands
        .iter()
        .filter(|command| command.name.starts_with(typed) && command.name != typed)
        .take(COMPLETIONS)
        .collect()
}

/// Recalling earlier prompts: the session's own, sent from any client or
/// replayed from its history, then those still queued, newest last.
#[derive(Default)]
pub struct History {
    /// The prompts being stepped through, taken when recall starts so the
    /// transcript growing meanwhile does not shift them.
    prompts: Vec<String>,
    /// The index being shown, while the user steps through.
    at: Option<usize>,
    /// The prompt this view sent last, until the transcript shows it, so it
    /// recalls before the program echoes it, or when it refused it.
    sent: Option<String>,
}

impl History {
    /// Notes a prompt this view sent and ends recall.
    pub fn push(&mut self, prompt: &str) {
        self.at = None;
        self.sent = Some(prompt.to_owned());
    }

    pub fn recalling(&self) -> bool {
        self.at.is_some()
    }

    pub fn stop(&mut self) {
        self.at = None;
    }

    /// The prompt before the one shown, or the newest.
    pub fn older(&mut self, transcript: &Transcript) -> Option<&str> {
        let at = match self.at {
            Some(0) => 0,
            Some(at) => at - 1,
            None => {
                self.collect(transcript);
                self.prompts.len().checked_sub(1)?
            }
        };
        self.at = Some(at);
        self.prompts.get(at).map(String::as_str)
    }

    /// The prompt after the one shown, or nothing past the newest.
    pub fn newer(&mut self) -> Option<&str> {
        let at = self.at? + 1;
        if at >= self.prompts.len() {
            self.at = None;
            return Some("");
        }
        self.at = Some(at);
        self.prompts.get(at).map(String::as_str)
    }

    fn collect(&mut self, transcript: &Transcript) {
        let shown = transcript
            .entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                EntryKind::User { text } => Some(prompt_text(text)),
                _ => None,
            });
        let queued = transcript.queued.iter().map(|queued| queued.text.as_str());
        let mut prompts: Vec<String> = Vec::new();
        for prompt in shown.chain(queued) {
            if prompt.trim().is_empty() || prompts.last().is_some_and(|last| last == prompt) {
                continue;
            }
            prompts.push(prompt.to_owned());
        }
        if let Some(sent) = &self.sent {
            if prompts.contains(sent) {
                self.sent = None;
            } else {
                prompts.push(sent.clone());
            }
        }
        let skip = prompts.len().saturating_sub(HISTORY);
        prompts.drain(..skip);
        self.prompts = prompts;
    }
}

/// A prompt as the user typed it: the transcript's text without the lines
/// naming the files it carried.
fn prompt_text(shown: &str) -> &str {
    let mut text = shown;
    while let Some((before, line)) = text.rsplit_once('\n') {
        if !line.starts_with(ATTACHED) {
            break;
        }
        text = before;
    }
    if text.starts_with(ATTACHED) && !text.contains('\n') {
        return "";
    }
    text
}

/// The command that moves `option` one choice along (`step` of 1 or -1),
/// or flips it when it is a toggle.
pub fn step(option: &ConfigOption, step: isize) -> Option<Command> {
    let value = match &option.kind {
        OptionKind::Select { choices, current } => {
            if choices.len() < 2 {
                return None;
            }
            let at = choices.iter().position(|choice| &choice.id == current);
            let len = choices.len() as isize;
            let next = at.map_or(0, |at| (at as isize + step).rem_euclid(len)) as usize;
            OptionValue::Choice {
                id: choices.get(next)?.id.clone(),
            }
        }
        OptionKind::Toggle { current } => OptionValue::Toggle { value: !current },
    };
    Some(Command::SetOption {
        id: option.id.clone(),
        value,
    })
}

/// The next session mode: the agent's mode setting when it has one, else
/// its older list of modes.
pub fn next_mode(transcript: &Transcript) -> Option<Command> {
    if let Some(option) = transcript.option(OptionCategory::Mode) {
        return step(option, 1);
    }
    let modes = transcript.modes.as_ref()?;
    let index = modes
        .available
        .iter()
        .position(|mode| mode.id == modes.current)
        .map_or(0, |index| (index + 1) % modes.available.len());
    let mode = modes.available.get(index)?;
    Some(Command::SetMode {
        id: mode.id.clone(),
    })
}

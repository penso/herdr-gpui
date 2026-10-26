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
    sent: Option<Sent>,
}

struct Sent {
    prompt: String,
    /// How many prompts the transcript held when it was sent; only one
    /// after them can be its echo, not an earlier prompt with its text.
    after: usize,
}

impl History {
    /// Notes a prompt this view sent to the session `transcript` mirrors,
    /// and ends recall.
    pub fn push(&mut self, prompt: &str, transcript: &Transcript) {
        self.at = None;
        self.sent = Some(Sent {
            prompt: prompt.to_owned(),
            after: prompts(transcript).count(),
        });
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
        let all: Vec<&str> = prompts(transcript).collect();
        if let Some(sent) = &self.sent
            && all
                .get(sent.after..)
                .is_some_and(|newer| newer.contains(&sent.prompt.as_str()))
        {
            self.sent = None;
        }
        let mut recalled: Vec<String> = Vec::new();
        let sent = self.sent.as_ref().map(|sent| sent.prompt.as_str());
        for prompt in all.into_iter().chain(sent) {
            if prompt.trim().is_empty() || recalled.last().is_some_and(|last| last == prompt) {
                continue;
            }
            recalled.push(prompt.to_owned());
        }
        let skip = recalled.len().saturating_sub(HISTORY);
        recalled.drain(..skip);
        self.prompts = recalled;
    }
}

/// Every prompt in the session, as the user typed it, oldest first: those
/// the transcript shows, then those still queued. A queued prompt moves to
/// the transcript when sent, so each keeps its place in the order.
fn prompts(transcript: &Transcript) -> impl Iterator<Item = &str> {
    let shown = transcript
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, files } => Some(typed(text, *files)),
            _ => None,
        });
    shown.chain(transcript.queued.iter().map(|queued| queued.text.as_str()))
}

/// A prompt's text without the `files` lines the transcript added after it,
/// one per file it carried.
fn typed(shown: &str, files: usize) -> &str {
    let mut text = shown;
    for _ in 0..files {
        match text.rsplit_once('\n') {
            Some((before, line)) if line.starts_with(ATTACHED) => text = before,
            None if text.starts_with(ATTACHED) => return "",
            _ => break,
        }
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

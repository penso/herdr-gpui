//! The in-pane view's state: the transcript it mirrors, the composer, and
//! what keys do. Pure, so it can be tested without a terminal.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use herdr_chat_protocol::{
    Attachment, Change, Command, Cursor, EntryId, MAX_ATTACHMENTS, Rejection, ServerMessage,
    Status, Transcript,
};
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

use herdr_chat_protocol::assist::{self, History};

use super::lines;

/// What the view asks the program to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Out {
    Command(Command),
    Quit,
}

/// What a message from the session meant for the view.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Received {
    Applied,
    /// The view lost track and must attach again for a snapshot.
    Resync,
    /// The session ended.
    Closed,
}

#[derive(Default)]
pub(crate) struct Composer {
    /// Byte offset of the cursor in `text`.
    pub cursor: usize,
    pub text: String,
}

impl Composer {
    fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .chars()
            .next()
            .map_or(self.cursor, |ch| self.cursor + ch.len_utf8())
    }

    fn backspace(&mut self) {
        let start = self.previous();
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    fn delete(&mut self) {
        let end = self.next();
        self.text.drain(self.cursor..end);
    }

    fn delete_word(&mut self) {
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end_matches(' ');
        let start = trimmed.rfind([' ', '\n']).map_or(0, |index| index + 1);
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Replaces the text, with the cursor at its end.
    fn set(&mut self, text: String) {
        self.cursor = text.len();
        self.text = text;
    }

    fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }

    /// The composer's rows at `width` columns, and the cursor's row and column.
    pub(crate) fn layout(&self, width: usize) -> (Vec<String>, (usize, usize)) {
        let width = width.max(1);
        let mut rows = vec![String::new()];
        let mut column = 0;
        let mut cursor = (0, 0);
        for (index, ch) in self.text.char_indices() {
            if index == self.cursor {
                cursor = (rows.len() - 1, column);
            }
            if ch == '\n' {
                rows.push(String::new());
                column = 0;
                continue;
            }
            let ch_width = ch.width().unwrap_or(0);
            if column + ch_width > width {
                rows.push(String::new());
                column = 0;
            }
            if let Some(row) = rows.last_mut() {
                row.push(ch);
            }
            column += ch_width;
        }
        if self.cursor >= self.text.len() {
            if column >= width {
                rows.push(String::new());
                column = 0;
            }
            cursor = (rows.len() - 1, column);
        }
        (rows, cursor)
    }
}

#[derive(Default)]
pub(crate) struct View {
    /// Files dragged in, which the next prompt carries.
    pub attachments: Vec<Attachment>,
    cache: HashMap<EntryId, Vec<Line<'static>>>,
    cache_width: usize,
    /// Which option of the oldest permission request is selected.
    pub choice: usize,
    pub composer: Composer,
    cursor: Option<Cursor>,
    /// Which of the composer's command completions is selected.
    pub completion: usize,
    /// A short message about the last thing that did not work.
    pub flash: Option<String>,
    history: History,
    quit_armed: bool,
    /// Lines scrolled up from the bottom; 0 follows new output.
    pub scroll: usize,
    /// The settings panel, open on the selected setting.
    pub settings: Option<usize>,
    pub transcript: Transcript,
}

impl View {
    pub(crate) fn cursor(&self) -> Option<Cursor> {
        self.cursor
    }

    pub(crate) fn receive(&mut self, message: ServerMessage) -> Received {
        match message {
            ServerMessage::Welcome { .. } => {}
            ServerMessage::Snapshot { cursor, transcript } => {
                self.transcript = *transcript;
                self.cursor = Some(cursor);
                self.cache.clear();
            }
            ServerMessage::Change { change, cursor } => {
                let follows = self.cursor.is_some_and(|known| {
                    known.epoch == cursor.epoch && known.seq + 1 == cursor.seq
                });
                if !follows || self.transcript.apply(&change).is_err() {
                    self.cursor = None;
                    return Received::Resync;
                }
                self.cursor = Some(cursor);
                self.invalidate(&change);
            }
            ServerMessage::Ack { rejected, .. } => {
                self.flash = rejected.map(|rejected: Rejection| rejected.to_string());
            }
            ServerMessage::Bye { .. } => return Received::Closed,
        }
        if self.choice >= self.options() {
            self.choice = 0;
        }
        Received::Applied
    }

    fn invalidate(&mut self, change: &Change) {
        match change {
            Change::Push { entry } | Change::Replace { entry } => {
                self.cache.remove(&entry.id);
            }
            Change::Append { id, .. } => {
                self.cache.remove(id);
            }
            Change::Truncate { before } => self.cache.retain(|id, _| id >= before),
            _ => {}
        }
    }

    fn options(&self) -> usize {
        self.transcript
            .permissions
            .first()
            .map_or(0, |permission| permission.options.len())
    }

    /// Every transcript line at `width` columns, from cached entries.
    pub(crate) fn lines(&mut self, width: usize) -> Vec<Line<'static>> {
        if width != self.cache_width {
            self.cache.clear();
            self.cache_width = width;
        }
        let mut out = Vec::new();
        if self.transcript.truncated {
            out.push(Line::styled(
                "  (earlier messages were dropped)",
                ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
            ));
        }
        for entry in &self.transcript.entries {
            let lines = self
                .cache
                .entry(entry.id)
                .or_insert_with(|| lines::entry(entry, width));
            out.extend(lines.iter().cloned());
        }
        out
    }

    pub(crate) fn paste(&mut self, text: &str) {
        self.quit_armed = false;
        if let Some(files) = super::dropped::files(text) {
            for path in files {
                if self.attachments.len() >= MAX_ATTACHMENTS {
                    self.flash = Some(format!("A prompt takes at most {MAX_ATTACHMENTS} files"));
                    break;
                }
                let attachment = Attachment { path };
                if !self.attachments.contains(&attachment) {
                    self.attachments.push(attachment);
                }
            }
            return;
        }
        self.composer
            .insert(&text.replace("\r\n", "\n").replace('\r', "\n"));
    }

    /// The agent's commands the composer's text would complete to.
    pub(crate) fn completions(&self) -> Vec<&herdr_chat_protocol::SlashCommand> {
        assist::completions(&self.transcript, &self.composer.text)
    }

    /// Keys while the settings panel is open: it takes every key.
    fn settings_key(&mut self, key: &KeyEvent, at: usize) -> Option<Out> {
        let count = self.transcript.options.len();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.settings = None,
            KeyCode::Char('o') if ctrl => self.settings = None,
            KeyCode::Up => self.settings = Some(at.saturating_sub(1)),
            KeyCode::Down => self.settings = Some((at + 1).min(count.saturating_sub(1))),
            KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                let step = if key.code == KeyCode::Left { -1 } else { 1 };
                let option = self.transcript.options.get(at)?;
                return assist::step(option, step).map(Out::Command);
            }
            _ => {}
        }
        None
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> Option<Out> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if !(ctrl && key.code == KeyCode::Char('c')) {
            self.quit_armed = false;
        }
        if let Some(at) = self.settings {
            return self.settings_key(&key, at);
        }
        if ctrl && key.code == KeyCode::Char('o') && !self.transcript.options.is_empty() {
            self.settings = Some(0);
            return None;
        }
        let completions = self.completions().len();
        if completions > 0 {
            match key.code {
                KeyCode::Up => {
                    self.completion = self.completion.saturating_sub(1);
                    return None;
                }
                KeyCode::Down => {
                    self.completion = (self.completion + 1).min(completions - 1);
                    return None;
                }
                KeyCode::Tab => {
                    let name = self.completions().get(self.completion)?.name.clone();
                    self.composer.text = format!("/{name} ");
                    self.composer.cursor = self.composer.text.len();
                    self.completion = 0;
                    return None;
                }
                _ => {}
            }
        }
        let empty = self.composer.text.is_empty();
        let asking = !self.transcript.permissions.is_empty();
        let recall = (empty || self.history.recalling()) && !asking && !shift;
        match key.code {
            KeyCode::Up if recall => {
                if let Some(prompt) = self.history.older(&self.transcript).map(str::to_owned) {
                    self.composer.set(prompt);
                }
                return None;
            }
            KeyCode::Down if recall && self.history.recalling() => {
                if let Some(prompt) = self.history.newer().map(str::to_owned) {
                    self.composer.set(prompt);
                }
                return None;
            }
            _ => {}
        }
        if !matches!(key.code, KeyCode::Up | KeyCode::Down) {
            self.history.stop();
        }
        if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) {
            self.completion = 0;
        }
        match key.code {
            KeyCode::Char('c') if ctrl => return self.interrupt(),
            KeyCode::Char('d') if ctrl && empty => return Some(Out::Quit),
            KeyCode::Char('j') if ctrl => self.composer.insert("\n"),
            KeyCode::Enter if alt || shift => self.composer.insert("\n"),
            KeyCode::Enter if asking && empty => return self.answer(self.choice),
            KeyCode::Enter => {
                if self.composer.text.trim().is_empty() && self.attachments.is_empty() {
                    return None;
                }
                self.scroll = 0;
                let text = self.composer.take();
                if !text.trim().is_empty() {
                    self.history.push(&text, &self.transcript);
                }
                return Some(Out::Command(Command::Prompt {
                    text,
                    attachments: std::mem::take(&mut self.attachments),
                }));
            }
            KeyCode::Char(digit @ '1'..='9') if asking && empty => {
                return self.answer(digit as usize - '1' as usize);
            }
            KeyCode::Up if asking && empty => self.choice = self.choice.saturating_sub(1),
            KeyCode::Down if asking && empty => {
                self.choice = (self.choice + 1).min(self.options().saturating_sub(1));
            }
            KeyCode::Esc => return self.cancel(),
            KeyCode::BackTab => return self.next_mode(),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Up => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Backspace if alt => self.composer.delete_word(),
            // From an empty composer, Backspace takes back the last file.
            KeyCode::Backspace if empty && !self.attachments.is_empty() => {
                self.attachments.pop();
            }
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Delete => self.composer.delete(),
            KeyCode::Left => self.composer.cursor = self.composer.previous(),
            KeyCode::Right => self.composer.cursor = self.composer.next(),
            KeyCode::Home => self.composer.cursor = 0,
            KeyCode::End => self.composer.cursor = self.composer.text.len(),
            KeyCode::Char('a') if ctrl => self.composer.cursor = 0,
            KeyCode::Char('e') if ctrl => self.composer.cursor = self.composer.text.len(),
            KeyCode::Char('u') if ctrl => {
                self.composer.text.drain(..self.composer.cursor);
                self.composer.cursor = 0;
            }
            KeyCode::Char('k') if ctrl => self.composer.text.truncate(self.composer.cursor),
            KeyCode::Char('w') if ctrl => self.composer.delete_word(),
            KeyCode::Char(ch) if !ctrl => {
                let mut buffer = [0; 4];
                self.composer.insert(ch.encode_utf8(&mut buffer));
            }
            _ => {}
        }
        None
    }

    fn working(&self) -> bool {
        matches!(self.transcript.status, Status::Working) || !self.transcript.queued.is_empty()
    }

    fn cancel(&mut self) -> Option<Out> {
        self.working().then_some(Out::Command(Command::Cancel))
    }

    /// Ctrl+C clears the prompt, then cancels the turn, then asks to be pressed
    /// again to quit.
    fn interrupt(&mut self) -> Option<Out> {
        if !self.composer.text.is_empty() {
            self.composer.take();
            return None;
        }
        if self.working() {
            return self.cancel();
        }
        if self.quit_armed {
            return Some(Out::Quit);
        }
        self.quit_armed = true;
        self.flash = Some("Press Ctrl+C again to quit".to_owned());
        None
    }

    fn answer(&mut self, index: usize) -> Option<Out> {
        let permission = self.transcript.permissions.first()?;
        let option = permission.options.get(index)?;
        Some(Out::Command(Command::Answer {
            option: option.id.clone(),
            permission: permission.id,
        }))
    }

    fn next_mode(&mut self) -> Option<Out> {
        assist::next_mode(&self.transcript).map(Out::Command)
    }
}

/// The program status this view reports to the terminal (OSC 7501).
pub(crate) fn program_status(transcript: &Transcript) -> &'static str {
    if transcript.is_blocked() {
        return "blocked:kind=permission";
    }
    match transcript.status {
        Status::Starting | Status::Working => "working",
        Status::Idle => "idle",
        Status::Failed { .. } => "error",
        Status::Exited { .. } => "done",
    }
}

#[cfg(test)]
mod tests;

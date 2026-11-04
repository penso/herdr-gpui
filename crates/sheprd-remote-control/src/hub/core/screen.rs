//! Emulate raw output once per visited pane, shared by paired phones.

use crate::hub::{ApiMethod, Error, Result};
use serde_json::{Value, json};
use sheprd_client::protocol::{CellData, ClientKeyCode, ClientKeyKind, ClientPaneInputEvent};
use sheprd_core_client::{
    Attachment, Core, End,
    wire::{Attach, Terminal},
};
use sheprd_terminal::{Emulator, Size};
use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

pub(super) struct Screen {
    attachment: Attachment,
    emulator: Emulator,
    lost: bool,
    sync_since: Option<Instant>,
    touched: Instant,
}

impl Screen {
    pub(super) fn open(core: &Core, terminal: &Terminal) -> Result<Self> {
        let size = Size::new(terminal.cols, terminal.rows).map_err(|_| Error::Response)?;
        let attachment = core.attach(&Attach {
            cols: size.cols(),
            rows: size.rows(),
            terminal: &terminal.address,
            ..Attach::default()
        })?;
        Ok(Self {
            attachment,
            emulator: Emulator::new(size, 0),
            lost: false,
            sync_since: None,
            touched: Instant::now(),
        })
    }

    pub(super) fn expired(&self) -> bool {
        self.touched.elapsed() > Duration::from_secs(30) || self.lost
    }

    pub(super) fn follow_size(&mut self, terminal: &Terminal) -> Result<()> {
        let size = Size::new(terminal.cols, terminal.rows).map_err(|_| Error::Response)?;
        if size != self.emulator.size() {
            self.emulator.resize(size);
        }
        Ok(())
    }

    pub(super) fn call(&mut self, method: ApiMethod, params: &Value) -> Result<Value> {
        self.touched = Instant::now();
        self.drain()?;
        match method {
            ApiMethod::PaneRead => Ok(
                json!({"read": {"text": ansi(&self.emulator.frame().cells, self.emulator.size().cols())}}),
            ),
            ApiMethod::PaneSendText => {
                let text = params["text"].as_str().ok_or(Error::Response)?;
                self.send(ClientPaneInputEvent::Paste(text.to_owned()))?;
                Ok(json!({}))
            }
            ApiMethod::PaneSendKeys => {
                for key in params["keys"].as_array().ok_or(Error::Response)? {
                    self.send(key_event(key.as_str().ok_or(Error::Response)?)?)?;
                }
                Ok(json!({}))
            }
            _ => Err(Error::Response),
        }
    }

    fn send(&self, event: ClientPaneInputEvent) -> Result<()> {
        if self.lost {
            return Err(super::Error::Disconnected.into());
        }
        self.attachment.write(&self.emulator.encode(&event))?;
        Ok(())
    }

    fn drain(&mut self) -> Result<()> {
        if self.lost {
            return Err(super::Error::Disconnected.into());
        }
        // A continuously writing terminal must not hold the shared source lock.
        let mut bytes = 0;
        loop {
            let chunk = self.attachment.read(Duration::ZERO)?;
            self.emulator.feed(&chunk.bytes);
            bytes += chunk.bytes.len();
            if matches!(chunk.end, Some(End::Lost | End::Closed(_))) {
                self.lost = true;
            }
            if chunk.bytes.is_empty() || chunk.end.is_some() || bytes >= 1024 * 1024 {
                break;
            }
        }
        if self.emulator.sync_pending() {
            let since = self.sync_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(150) {
                self.emulator.end_sync();
                self.sync_since = None;
            }
        } else {
            self.sync_since = None;
        }
        self.emulator.take_events();
        Ok(())
    }
}

fn key_event(key: &str) -> Result<ClientPaneInputEvent> {
    let mut parts: Vec<&str> = key.split('+').collect();
    let name = parts.pop().ok_or(Error::Response)?;
    let mut modifiers = 0;
    for part in parts {
        modifiers |= match part {
            "ctrl" => sheprd_terminal::input::CONTROL,
            "alt" => sheprd_terminal::input::ALT,
            "shift" => sheprd_terminal::input::SHIFT,
            _ => return Err(Error::Response),
        };
    }
    let code = match name {
        "backspace" => ClientKeyCode::Backspace,
        "down" => ClientKeyCode::Down,
        "enter" => ClientKeyCode::Enter,
        "esc" => ClientKeyCode::Esc,
        "left" => ClientKeyCode::Left,
        "right" => ClientKeyCode::Right,
        "tab" => ClientKeyCode::Tab,
        "up" => ClientKeyCode::Up,
        "backtick" => ClientKeyCode::Char('`'),
        "minus" => ClientKeyCode::Char('-'),
        "plus" => ClientKeyCode::Char('+'),
        "slash" => ClientKeyCode::Char('/'),
        "space" => ClientKeyCode::Char(' '),
        name if name.starts_with('f') && name.len() > 1 => {
            let number = name[1..].parse::<u8>().map_err(|_| Error::Response)?;
            if !(1..=12).contains(&number) {
                return Err(Error::Response);
            }
            ClientKeyCode::F(number)
        }
        name if name.chars().count() == 1 => {
            ClientKeyCode::Char(name.chars().next().ok_or(Error::Response)?)
        }
        _ => return Err(Error::Response),
    };
    Ok(ClientPaneInputEvent::Key {
        code,
        generated_text: None,
        kind: ClientKeyKind::Press,
        modifiers,
        physical_key_id: None,
        repeat_count: 1,
        shifted_codepoint: None,
        tracks_release: false,
        windows_record: None,
    })
}

fn ansi(cells: &[CellData], cols: u16) -> String {
    let mut text = String::new();
    let mut style = None;
    for row in cells.chunks(usize::from(cols)) {
        if !text.is_empty() {
            text.push_str("\x1b[0m\n");
            style = None;
        }
        for cell in row.iter().filter(|cell| !cell.skip) {
            let next = (cell.fg, cell.bg, cell.modifier);
            if style != Some(next) {
                text.push_str("\x1b[0");
                for (bit, code) in [(1, 1), (2, 2), (4, 3), (8, 4), (64, 7), (128, 8), (256, 9)] {
                    if cell.modifier & bit != 0 {
                        let _ = write!(text, ";{code}");
                    }
                }
                color(&mut text, cell.fg, false);
                color(&mut text, cell.bg, true);
                text.push('m');
                style = Some(next);
            }
            text.extend(cell.symbol.chars().filter(|c| !c.is_control()));
        }
    }
    text.push_str("\x1b[0m");
    text
}

fn color(text: &mut String, value: u32, background: bool) {
    let prefix = if background { 48 } else { 38 };
    match value >> 24 {
        1 => {
            let _ = write!(text, ";{prefix};5;{}", value & 255);
        }
        2 => {
            let _ = write!(
                text,
                ";{prefix};2;{};{};{}",
                (value >> 16) & 255,
                (value >> 8) & 255,
                value & 255
            );
        }
        _ if (1..=16).contains(&value) => {
            let index = value - 1;
            let base = if background { 40 } else { 30 };
            let _ = write!(
                text,
                ";{}",
                base + index % 8 + if index >= 8 { 60 } else { 0 }
            );
        }
        _ => {}
    }
}

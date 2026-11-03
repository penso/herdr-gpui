use crate::{Attachment, Core, End, Error, Result, wire::Attach};
use herdr_backend::{
    ClientMouseKind, ClientPaneInputEvent, Exit, KeyReporting, Role, Size, SurfaceUpdate,
    TerminalId,
};
use herdr_terminal::{Damage, Emulator, Event, MouseReporting, Scroll};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How long a synchronized update (DEC 2026) may hold the screen before it
/// is shown anyway, as terminals bound it.
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);
/// How often an observer asks the core for its terminal's size, which the
/// active viewer sets and nothing on an attachment reports.
const SIZE_CHECK: Duration = Duration::from_millis(500);

/// An attached herdr-core terminal, emulated here: its raw output becomes
/// cells, and semantic input becomes bytes for its program's modes.
pub struct CoreTerminal {
    attachment: Attachment,
    emulator: Emulator,
    end: Option<End>,
    /// The key events the program asked for when the view was last told.
    keys: Option<KeyReporting>,
    /// Whether the program read the mouse when the view was last told.
    reporting: Option<bool>,
    role: Role,
    /// An observer's core and when it last read the terminal's size there;
    /// an active view sets the size itself.
    sizing: Option<(Arc<Core>, Instant)>,
    /// When the synchronized update the emulator holds began.
    sync_since: Option<Instant>,
}

impl CoreTerminal {
    pub(super) fn attach(
        core: &Arc<Core>,
        id: &TerminalId,
        size: Size,
        role: Role,
        scrollback: usize,
    ) -> Result<Self> {
        let attachment = core.attach(&Attach {
            cols: size.cols(),
            observe: role == Role::Observer,
            rows: size.rows(),
            terminal: id.as_str(),
            ..Attach::default()
        })?;
        // An observer sees the screen at the size its active viewer set.
        let emulated = match role {
            Role::Active => size,
            Role::Observer => {
                Size::new(attachment.terminal.cols, attachment.terminal.rows).unwrap_or(size)
            }
        };
        Ok(Self {
            attachment,
            emulator: Emulator::new(emulated, scrollback),
            end: None,
            keys: None,
            reporting: None,
            role,
            sizing: (role == Role::Observer).then(|| (Arc::clone(core), Instant::now())),
            sync_since: None,
        })
    }

    /// Keeps an observer's screen at the size the active viewer set, read
    /// from the core now and then. A failed read leaves the size as it is;
    /// a lost connection ends the attachment too, which reports it.
    fn follow_size(&mut self) {
        let Some((core, checked)) = &mut self.sizing else {
            return;
        };
        if checked.elapsed() < SIZE_CHECK {
            return;
        }
        *checked = Instant::now();
        let address = &self.attachment.terminal.address;
        let size = match core.list(false) {
            Ok(list) => list
                .terminals
                .iter()
                .find(|terminal| terminal.address == *address)
                .and_then(|terminal| Size::new(terminal.cols, terminal.rows).ok()),
            Err(error) => {
                tracing::debug!(%error, "herdr-core observer size unread");
                None
            }
        };
        if let Some(size) = size
            && size != self.emulator.size()
        {
            self.emulator.resize(size);
        }
    }

    fn drives(&self) -> bool {
        self.role == Role::Active
    }

    fn release_stale_sync(&mut self) {
        if !self.emulator.sync_pending() {
            self.sync_since = None;
            return;
        }
        let since = *self.sync_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= SYNC_TIMEOUT {
            self.emulator.end_sync();
            self.sync_since = None;
        }
    }
}

impl herdr_backend::Terminal for CoreTerminal {
    type Error = Error;

    /// A wheel turn the program does not read scrolls this view's history,
    /// an observer's too. Typing returns the view to the live screen.
    fn input(&mut self, input: &ClientPaneInputEvent) -> Result<()> {
        let bytes = self.emulator.encode(input);
        if bytes.is_empty() {
            if let ClientPaneInputEvent::Mouse { kind, lines, .. } = input {
                let lines = i32::from((*lines).max(1));
                match kind {
                    ClientMouseKind::ScrollUp => self.emulator.scroll(Scroll::Lines(lines)),
                    ClientMouseKind::ScrollDown => self.emulator.scroll(Scroll::Lines(-lines)),
                    _ => {}
                }
            }
            return Ok(());
        }
        if !self.drives() {
            return Ok(());
        }
        if !matches!(input, ClientPaneInputEvent::Mouse { .. })
            && self.emulator.scroll_metrics().offset > 0
        {
            self.emulator.scroll(Scroll::Bottom);
        }
        self.attachment.write(&bytes)
    }

    /// An active view sizes the program's terminal. An observer keeps the
    /// size the program draws at, whatever its own view's bounds, so its
    /// output wraps where the program's does.
    fn resize(&mut self, size: Size) -> Result<()> {
        if !self.drives() {
            return Ok(());
        }
        self.emulator.resize(size);
        self.attachment.resize(size.cols(), size.rows())
    }

    /// The core reports focus to programs that asked for it.
    fn focus(&mut self, focused: bool) -> Result<()> {
        if !self.drives() {
            return Ok(());
        }
        self.attachment.focus(focused)
    }

    fn next(&mut self, timeout: Duration) -> Result<Vec<SurfaceUpdate>> {
        match &self.end {
            Some(End::Lost) => return Err(Error::Lost),
            Some(End::Closed(_)) => return Ok(Vec::new()),
            None => {}
        }
        // A held update is released on time even when no bytes follow it.
        let timeout = match self.sync_since {
            Some(since) => {
                timeout.min((since + SYNC_TIMEOUT).saturating_duration_since(Instant::now()))
            }
            None => timeout,
        };
        self.follow_size();
        let chunk = self.attachment.read(timeout)?;
        self.emulator.feed(&chunk.bytes);
        self.release_stale_sync();
        let mut updates: Vec<SurfaceUpdate> = self
            .emulator
            .take_events()
            .into_iter()
            .map(|event| match event {
                Event::Bell { .. } => SurfaceUpdate::Bell,
                Event::Title(title) => SurfaceUpdate::Title(title.unwrap_or_default()),
            })
            .collect();
        let modes = self.emulator.modes();
        let keys = KeyReporting {
            every_key: modes.reports_every_key(),
            releases: modes.reports_key_releases(),
        };
        if self.keys != Some(keys) {
            self.keys = Some(keys);
            updates.push(SurfaceUpdate::KeyReporting(keys));
        }
        let reporting = modes.mouse != MouseReporting::Off;
        if self.reporting != Some(reporting) {
            self.reporting = Some(reporting);
            updates.push(SurfaceUpdate::MouseReporting(reporting));
        }
        match self.emulator.take_damage() {
            Damage::Full(frame) => updates.push(SurfaceUpdate::Frame(frame)),
            damage if damage.is_empty() => {}
            Damage::Rows { rows, cursor } => updates.push(SurfaceUpdate::Rows { cursor, rows }),
        }
        match chunk.end {
            Some(End::Closed(closed)) => {
                updates.push(SurfaceUpdate::Ended(Exit {
                    code: closed.exit_code,
                }));
                self.end = Some(End::Closed(closed));
            }
            // What arrived before the loss is still shown; the next call fails.
            Some(End::Lost) if updates.is_empty() => return Err(Error::Lost),
            Some(End::Lost) => self.end = Some(End::Lost),
            None => {}
        }
        Ok(updates)
    }
}

//! The emulator: alacritty's `Term` and vte's parser behind a small, pure,
//! deterministic API. It never writes to the PTY, opens links, or touches the
//! clipboard; herdr-core answers terminal queries itself.

use crate::{
    MAX_SCROLLBACK_CELLS, MAX_SCROLLBACK_LINES, Size, cells,
    damage::{Damage, Tracker},
    listener::Listener,
    modes::Modes,
    scan::Scanner,
};
use alacritty_terminal::{
    Term,
    grid::{self, Dimensions},
    term::{Config, Osc52, TermMode},
    vte::ansi::{CursorShape, CursorStyle, Processor, Timeout},
};
use herdr_protocol::{ClientPaneInputEvent, FrameData};
use std::{fmt, time::Duration};

/// Something the program did that is not on screen. Taking events coalesces
/// them: at most one title change and one bell count per take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The bell rang `count` times.
    Bell { count: u32 },
    /// The title changed; `None` is a reset to the default.
    Title(Option<String>),
}

/// A move of the view through history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    /// Positive lines move up into history, negative back down.
    Lines(i32),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

impl From<Scroll> for grid::Scroll {
    fn from(scroll: Scroll) -> Self {
        match scroll {
            Scroll::Lines(lines) => Self::Delta(lines),
            Scroll::PageUp => Self::PageUp,
            Scroll::PageDown => Self::PageDown,
            Scroll::Top => Self::Top,
            Scroll::Bottom => Self::Bottom,
        }
    }
}

/// Where the view is in history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollMetrics {
    /// Lines the view is scrolled up from the live screen; 0 is live.
    pub offset: usize,
    /// History lines above the live screen.
    pub history: usize,
    /// Rows the view shows.
    pub rows: u16,
}

/// Synchronized updates (DECSET 2026) without a clock: the parser buffers
/// until the program ends the update, vte's 2 MiB cap, or the caller ends it
/// with [`Emulator::end_sync`], so time never changes what a feed does.
#[derive(Debug, Default)]
struct SyncPending(bool);

impl Timeout for SyncPending {
    fn set_timeout(&mut self, _: Duration) {
        self.0 = true;
    }

    fn clear_timeout(&mut self) {
        self.0 = false;
    }

    fn pending_timeout(&self) -> bool {
        self.0
    }
}

struct Dims(Size);

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.0.rows())
    }

    fn columns(&self) -> usize {
        usize::from(self.0.cols())
    }
}

pub struct Emulator {
    bells: u32,
    damage: Tracker,
    listener: Listener,
    processor: Processor<SyncPending>,
    scanner: Scanner,
    scrollback_lines: usize,
    size: Size,
    term: Term<Listener>,
    title: Option<String>,
    title_changed: bool,
}

impl fmt::Debug for Emulator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Emulator")
            .field("size", &self.size)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl Emulator {
    /// A blank terminal keeping up to `scrollback_lines` of history, clamped
    /// to [`MAX_SCROLLBACK_LINES`] and [`MAX_SCROLLBACK_CELLS`].
    pub fn new(size: Size, scrollback_lines: usize) -> Self {
        let listener = Listener::default();
        let config = config(size, scrollback_lines);
        let term = Term::new(config, &Dims(size), listener.clone());
        Self {
            bells: 0,
            damage: Tracker::default(),
            listener,
            processor: Processor::new(),
            scanner: Scanner::default(),
            scrollback_lines,
            size,
            term,
            title: None,
            title_changed: false,
        }
    }

    /// Runs PTY output through the terminal. How the bytes are split across
    /// calls never changes the result.
    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let Self {
            processor,
            scanner,
            term,
            ..
        } = self;
        scanner.feed(bytes, |run| processor.advance(term, run));
        self.damage.fed = true;
        self.drain_events();
    }

    /// Whether a synchronized update is holding back output.
    pub fn sync_pending(&self) -> bool {
        self.processor.sync_timeout().pending_timeout()
    }

    /// Applies a synchronized update the program has not ended, for a caller
    /// whose own timer ran out (terminals use about 150 ms).
    pub fn end_sync(&mut self) {
        if !self.sync_pending() {
            return;
        }
        self.processor.stop_sync(&mut self.term);
        self.damage.fed = true;
        self.drain_events();
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn resize(&mut self, size: Size) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.resize(Dims(size));
        self.term.set_options(config(size, self.scrollback_lines));
        self.damage.full = true;
    }

    /// The visible viewport, scrolled as [`Self::scroll`] left it. It does not
    /// reset damage: [`Damage::Full`] carries the frame later rows patch.
    pub fn frame(&self) -> FrameData {
        cells::frame(&self.term, &mut Vec::new())
    }

    /// What changed since the previous take; the first take is always full.
    pub fn take_damage(&mut self) -> Damage {
        let alt_screen = self.term.mode().contains(TermMode::ALT_SCREEN);
        self.damage.take(&mut self.term, alt_screen)
    }

    pub fn modes(&self) -> Modes {
        Modes::new(*self.term.mode(), self.scanner.win32_input())
    }

    /// The PTY bytes for `input` under the current modes.
    pub fn encode(&self, input: &ClientPaneInputEvent) -> Vec<u8> {
        crate::input::encode(input, &self.modes())
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn scroll(&mut self, scroll: Scroll) {
        let before = self.term.grid().display_offset();
        self.term.scroll_display(scroll.into());
        if self.term.grid().display_offset() != before {
            self.damage.full = true;
        }
    }

    pub fn scroll_metrics(&self) -> ScrollMetrics {
        let grid = self.term.grid();
        ScrollMetrics {
            offset: grid.display_offset(),
            history: grid.history_size(),
            rows: self.size.rows(),
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        if std::mem::take(&mut self.title_changed) {
            events.push(Event::Title(self.title.clone()));
        }
        let count = std::mem::take(&mut self.bells);
        if count > 0 {
            events.push(Event::Bell { count });
        }
        events
    }

    fn drain_events(&mut self) {
        let inbox = self.listener.take();
        if let Some(title) = inbox.title {
            self.title_changed |= title != self.title;
            self.title = title;
        }
        self.bells = self.bells.saturating_add(inbox.bells);
    }
}

fn config(size: Size, scrollback_lines: usize) -> Config {
    let cell_limit = MAX_SCROLLBACK_CELLS / usize::from(size.cols());
    Config {
        scrolling_history: scrollback_lines.min(MAX_SCROLLBACK_LINES).min(cell_limit),
        // Stands for "no program chose a style" (see `cells::cursor`).
        default_cursor_style: CursorStyle {
            shape: CursorShape::HollowBlock,
            blinking: false,
        },
        kitty_keyboard: true,
        osc52: Osc52::Disabled,
        ..Config::default()
    }
}

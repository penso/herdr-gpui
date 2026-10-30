//! The terminal modes a program set that change how the app encodes input.

use alacritty_terminal::term::TermMode;

/// Which mouse events the program asked for (DECSET 1000, 1002, 1003).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum MouseReporting {
    #[default]
    Off,
    /// Presses, releases, and the wheel (1000).
    Click,
    /// Also motion while a button is held (1002).
    Drag,
    /// Also motion with no button held (1003).
    Motion,
}

/// How mouse reports are encoded (DECSET 1005, 1006).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum MouseEncoding {
    /// `CSI M` with one byte per value, so positions stop at 222.
    #[default]
    X10,
    /// `CSI M` with UTF-8 values, so positions stop at 2014.
    Utf8,
    /// `CSI < b ; x ; y M/m`, with no position limit.
    Sgr,
}

/// The input modes in effect, read after every feed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modes {
    // keep-sorted start
    pub alt_screen: bool,
    /// Wheel turns become arrow keys on the alternate screen (1007).
    pub alternate_scroll: bool,
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub focus_reporting: bool,
    /// The kitty keyboard protocol's progressive enhancement flags
    /// (1 disambiguate, 2 event types, 4 alternate keys, 8 all keys as
    /// escapes, 16 associated text); 0 is legacy encoding.
    pub kitty_keyboard: u8,
    /// Enter sends CR LF (LNM).
    pub line_feed_new_line: bool,
    pub mouse: MouseReporting,
    pub mouse_encoding: MouseEncoding,
    /// Win32 input mode (DECSET 9001), which alacritty does not know.
    pub win32_input: bool,
    // keep-sorted end
}

impl Modes {
    /// Whether the program asked for key releases (kitty flag 2).
    pub fn reports_key_releases(&self) -> bool {
        self.kitty_keyboard & 2 != 0
    }

    /// Whether the program asked for every key as an escape, printable
    /// ones too (kitty flag 8).
    pub fn reports_every_key(&self) -> bool {
        self.kitty_keyboard & 8 != 0
    }

    pub(crate) fn new(mode: TermMode, win32_input: bool) -> Self {
        let mouse = if mode.contains(TermMode::MOUSE_MOTION) {
            MouseReporting::Motion
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            MouseReporting::Drag
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            MouseReporting::Click
        } else {
            MouseReporting::Off
        };
        let mouse_encoding = if mode.contains(TermMode::SGR_MOUSE) {
            MouseEncoding::Sgr
        } else if mode.contains(TermMode::UTF8_MOUSE) {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::X10
        };
        let kitty = [
            TermMode::DISAMBIGUATE_ESC_CODES,
            TermMode::REPORT_EVENT_TYPES,
            TermMode::REPORT_ALTERNATE_KEYS,
            TermMode::REPORT_ALL_KEYS_AS_ESC,
            TermMode::REPORT_ASSOCIATED_TEXT,
        ];
        let kitty_keyboard = kitty
            .iter()
            .enumerate()
            .filter(|(_, flag)| mode.contains(**flag))
            .fold(0, |flags, (bit, _)| flags | 1 << bit);
        Self {
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            app_keypad: mode.contains(TermMode::APP_KEYPAD),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            focus_reporting: mode.contains(TermMode::FOCUS_IN_OUT),
            kitty_keyboard,
            line_feed_new_line: mode.contains(TermMode::LINE_FEED_NEW_LINE),
            mouse,
            mouse_encoding,
            win32_input,
        }
    }
}

//! Default shortcuts on Linux and Windows. GPUI reads `cmd` as the Super or
//! Windows key there, which GNOME, KDE, and Windows keep for themselves
//! (Super-1..9 launches dock apps, Super-D shows the desktop, Super-Left
//! tiles the window), so the macOS catalog would mostly never arrive. These
//! follow GNOME Terminal, Terminator, and WezTerm instead: Ctrl-Shift for
//! app commands, Alt-digit for tabs, and Ctrl alone only where no terminal
//! program reads the key. Ctrl-Shift-C and Ctrl-Shift-V stay with the
//! terminal's clipboard, and Ctrl-Shift-U with IBus's Unicode entry.

use super::{Command, CommandInfo};

/// Which default table a keymap starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    Mac,
    Desktop,
}

impl Platform {
    /// Tests pin the macOS catalog so their keystrokes read the same on every
    /// host; `Desktop` is tested by naming it.
    pub(crate) const CURRENT: Self = if cfg!(any(target_os = "macos", test)) {
        Self::Mac
    } else {
        Self::Desktop
    };
}

impl CommandInfo {
    /// The keystrokes `command` starts with on `platform`, primary first.
    pub(crate) fn defaults(&self, platform: Platform) -> &'static [&'static str] {
        match platform {
            Platform::Mac => self.shortcuts,
            Platform::Desktop => desktop(self.command),
        }
    }
}

fn desktop(command: Command) -> &'static [&'static str] {
    use Command::*;
    match command {
        OpenNotificationTarget => &["ctrl-shift-j"],
        NewWindow => &["ctrl-shift-alt-n"],
        Workspace => &["ctrl-shift-n"],
        NewWorktree => &["ctrl-shift-g"],
        Tab => &["ctrl-shift-t"],
        SplitRight => &["ctrl-shift-e"],
        SplitDown => &["ctrl-shift-o"],
        NextTab => &["ctrl-pagedown", "ctrl-tab"],
        PreviousTab => &["ctrl-pageup", "ctrl-shift-tab"],
        FocusLeft => &["ctrl-shift-left"],
        FocusRight => &["ctrl-shift-right"],
        FocusUp => &["ctrl-shift-up"],
        FocusDown => &["ctrl-shift-down"],
        NextPane => &["ctrl-shift-]"],
        PreviousPane => &["ctrl-shift-["],
        Zoom => &["ctrl-shift-z"],
        ClearPane => &["ctrl-shift-k"],
        Find => &["ctrl-shift-f"],
        CopyMode => &["ctrl-shift-x"],
        ClosePane => &["ctrl-shift-w"],
        CloseTab => &["ctrl-shift-alt-w"],
        TabNumber(1) => &["alt-1"],
        TabNumber(2) => &["alt-2"],
        TabNumber(3) => &["alt-3"],
        TabNumber(4) => &["alt-4"],
        TabNumber(5) => &["alt-5"],
        TabNumber(6) => &["alt-6"],
        TabNumber(7) => &["alt-7"],
        TabNumber(8) => &["alt-8"],
        TabNumber(9) => &["alt-9"],
        ToggleSidebar => &["ctrl-shift-b"],
        IncreaseFontSize => &["ctrl-=", "ctrl-+"],
        DecreaseFontSize => &["ctrl--"],
        ResetFontSize => &["ctrl-0"],
        Settings => &["ctrl-,"],
        Keybinds => &["ctrl-shift-h"],
        Sessions => &["ctrl-shift-s"],
        WorkspacePicker => &["ctrl-shift-l"],
        Palette => &["ctrl-shift-p"],
        Quit => &["ctrl-shift-q"],
        NewBrowserTab => &["ctrl-shift-i"],
        SplitEditor => &["ctrl-shift-\\"],
        _ => &[],
    }
}

use crate::Command;

/// Linux reserves Super for the desktop and Ctrl for terminal programs.
/// Explicit defaults keep shifted Mac actions from collapsing onto the
/// same Ctrl-Shift key, and leave Ctrl-Shift-C/V available for the clipboard.
pub(super) fn defaults(command: Command) -> &'static [&'static str] {
    use Command::*;
    match command {
        OpenNotificationTarget => &["ctrl-alt-shift-u"],
        NewWindow => &["ctrl-alt-n"],
        Workspace => &["ctrl-alt-shift-n"],
        // Keep the frequent creation action on N; Ctrl-Shift-U is Unicode entry.
        NewWorktree => &["ctrl-shift-n"],
        Tab => &["ctrl-shift-t"],
        SplitRight => &["ctrl-shift-d"],
        SplitDown => &["ctrl-alt-shift-d"],
        NextTab => &["ctrl-pagedown"],
        PreviousTab => &["ctrl-pageup"],
        FocusLeft => &["ctrl-alt-shift-left"],
        FocusRight => &["ctrl-alt-shift-right"],
        FocusUp => &["ctrl-alt-shift-up"],
        FocusDown => &["ctrl-alt-shift-down"],
        NextPane => &["ctrl-alt-shift-]"],
        PreviousPane => &["ctrl-alt-shift-["],
        Back => &["ctrl-shift-["],
        Forward => &["ctrl-shift-]"],
        Zoom => &["ctrl-shift-enter"],
        ClearPane => &["ctrl-shift-k"],
        Find => &["ctrl-shift-f"],
        FindNext => &["ctrl-shift-g"],
        FindPrevious => &["ctrl-alt-shift-g"],
        CopyMode => &["ctrl-alt-shift-c"],
        AnnotateSelection => &["ctrl-alt-shift-a"],
        ClosePane => &["ctrl-shift-w"],
        CloseTab => &["ctrl-alt-shift-w"],
        // Ctrl-Shift-T is New Tab here, so its partner gains Alt.
        ReopenClosedTab => &["ctrl-alt-shift-t"],
        TabNumber(1) => &["ctrl-shift-1"],
        TabNumber(2) => &["ctrl-shift-2"],
        TabNumber(3) => &["ctrl-shift-3"],
        TabNumber(4) => &["ctrl-shift-4"],
        TabNumber(5) => &["ctrl-shift-5"],
        TabNumber(6) => &["ctrl-shift-6"],
        TabNumber(7) => &["ctrl-shift-7"],
        TabNumber(8) => &["ctrl-shift-8"],
        TabNumber(9) => &["ctrl-shift-9"],
        ToggleSidebar => &["ctrl-shift-b"],
        ToggleStatusBar => &["ctrl-alt-b"],
        // Windows' own key for moving between a window's regions.
        FocusNextRegion => &["f6"],
        FocusPreviousRegion => &["shift-f6"],
        ToggleZenMode => &["ctrl-alt-z"],
        // GPUI drops Shift from punctuation: the physical Ctrl-Shift keys
        // arrive as their symbols on Linux (for example Ctrl-Shift-- as Ctrl-_).
        IncreaseFontSize => &["ctrl-shift-=", "ctrl-shift-+", "ctrl-+"],
        DecreaseFontSize => &["ctrl-shift--", "ctrl-_"],
        ResetFontSize => &["ctrl-shift-0", "ctrl-)"],
        Settings => &["ctrl-shift-,"],
        Keybinds => &["ctrl-shift-/"],
        Sessions => &["ctrl-shift-s"],
        WorkspacePicker => &["ctrl-alt-shift-p"],
        Palette => &["ctrl-shift-p"],
        ResumePalette => &["ctrl-alt-shift-r"],
        GoToSymbol => &["ctrl-alt-shift-o"],
        GoToFile => &["ctrl-shift-o"],
        Quit => &["ctrl-shift-q"],
        NewBrowserTab => &["ctrl-alt-shift-b"],
        SplitEditor => &["ctrl-shift-\\"],
        ToggleFullScreen => &["f11"],
        CycleWindows => &["ctrl-shift-`"],
        // Tiling compositors such as Hyprland draw no close button.
        CloseWindow => &["ctrl-alt-w"],
        _ => &[],
    }
}

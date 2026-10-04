//! The Herdr daemon's `[keys]` table, read so the GUI answers the same
//! shortcuts, prefix chords included, as the TUI. Herdr owns that file and
//! reports its own mistakes, so anything this client cannot express (an
//! unparseable entry, a `hyper` modifier, an action with no GUI command) is
//! skipped rather than turned into a GUI config error.
//!
//! `open_worktree` and `remove_worktree` act on the focused workspace, as in
//! the TUI, by opening the dialog its menu row would; `detach` lets go of the
//! selected daemon, which keeps running, as the settings menu's row does.
//!
//! Herdr's `navigate_pane_*` keys have no GUI command: they move a pane
//! cursor in the TUI's navigator, and the workspace picker here is a search
//! field with no pane cursor to move.

use crate::{Error, Result, controls::Command};
use gpui::{Keystroke, Modifiers};

/// Herdr's own fallback when `prefix` is missing or names no usable key.
const DEFAULT_PREFIX: &str = "ctrl+b";

/// A daemon `[keys]` entry can hold a list, but never an unbounded one.
const MAX_ENTRIES: usize = 16;

/// A host's published profile is daemon data. Herdr's normalized `[keys]` is a
/// few kilobytes, so anything far larger is refused before it is parsed.
const MAX_PROFILE_BYTES: usize = 64 * 1024;

/// What a daemon action runs here.
#[derive(Clone, Copy)]
enum Target {
    Command(Command),
    /// An action expanded over the digits 1-9, each of which it binds
    /// selecting that item, as `[keys.indexed]` does too.
    Indexed(Indexed),
}

#[derive(Clone, Copy)]
enum Indexed {
    /// `switch_tab`, by the tab's number.
    Tab,
    /// `switch_workspace`, by position in the sidebar.
    Workspace,
    /// `focus_agent`, by position in the agent panel.
    Agent,
}

impl Indexed {
    fn command(self, digit: u8) -> Command {
        match self {
            Self::Tab => Command::TabNumber(digit),
            Self::Workspace => Command::WorkspaceNumber(digit),
            Self::Agent => Command::AgentNumber(digit),
        }
    }

    /// The `[keys.indexed]` entry naming a modifier combo for these digits.
    fn legacy_key(self) -> &'static str {
        match self {
            Self::Tab => "tabs",
            Self::Workspace => "workspaces",
            Self::Agent => "agents",
        }
    }
}

/// Daemon actions with a GUI equivalent, each with Herdr's default binding.
/// The module documentation lists the actions missing here, and why.
const ACTIONS: &[(&str, Target, &str)] = &[
    ("help", Target::Command(Command::Keybinds), "prefix+?"),
    ("settings", Target::Command(Command::Settings), "prefix+s"),
    (
        "new_workspace",
        Target::Command(Command::Workspace),
        "prefix+shift+n",
    ),
    (
        "new_worktree",
        Target::Command(Command::NewWorktree),
        "prefix+shift+g",
    ),
    ("open_worktree", Target::Command(Command::OpenWorktree), ""),
    (
        "remove_worktree",
        Target::Command(Command::RemoveWorktree),
        "",
    ),
    (
        "workspace_picker",
        Target::Command(Command::WorkspacePicker),
        "prefix+w",
    ),
    (
        "goto",
        Target::Command(Command::WorkspacePicker),
        "prefix+g",
    ),
    (
        "rename_workspace",
        Target::Command(Command::RenameWorkspace),
        "prefix+shift+w",
    ),
    (
        "close_workspace",
        Target::Command(Command::CloseWorkspace),
        "prefix+shift+d",
    ),
    ("detach", Target::Command(Command::Detach), ""),
    (
        "reload_config",
        Target::Command(Command::ReloadConfig),
        "prefix+shift+r",
    ),
    (
        "open_notification_target",
        Target::Command(Command::OpenNotificationTarget),
        "prefix+o",
    ),
    (
        "previous_workspace",
        Target::Command(Command::PreviousWorkspace),
        "",
    ),
    (
        "next_workspace",
        Target::Command(Command::NextWorkspace),
        "",
    ),
    (
        "previous_agent",
        Target::Command(Command::PreviousAgent),
        "",
    ),
    ("next_agent", Target::Command(Command::NextAgent), ""),
    ("focus_agent", Target::Indexed(Indexed::Agent), ""),
    ("new_tab", Target::Command(Command::Tab), "prefix+c"),
    (
        "previous_tab",
        Target::Command(Command::PreviousTab),
        "prefix+p",
    ),
    ("next_tab", Target::Command(Command::NextTab), "prefix+n"),
    (
        "rename_tab",
        Target::Command(Command::RenameTab),
        "prefix+shift+t",
    ),
    (
        "move_tab_previous",
        Target::Command(Command::MoveTabPrevious),
        "",
    ),
    ("move_tab_next", Target::Command(Command::MoveTabNext), ""),
    ("switch_tab", Target::Indexed(Indexed::Tab), "prefix+1..9"),
    ("switch_workspace", Target::Indexed(Indexed::Workspace), ""),
    (
        "close_tab",
        Target::Command(Command::CloseTab),
        "prefix+shift+x",
    ),
    (
        "rename_pane",
        Target::Command(Command::RenamePane),
        "prefix+shift+p",
    ),
    (
        "edit_scrollback",
        Target::Command(Command::EditScrollback),
        "prefix+e",
    ),
    ("clear_pane", Target::Command(Command::ClearPane), ""),
    ("copy_mode", Target::Command(Command::CopyMode), "prefix+["),
    (
        "focus_pane_left",
        Target::Command(Command::FocusLeft),
        "prefix+h",
    ),
    (
        "focus_pane_down",
        Target::Command(Command::FocusDown),
        "prefix+j",
    ),
    (
        "focus_pane_up",
        Target::Command(Command::FocusUp),
        "prefix+k",
    ),
    (
        "focus_pane_right",
        Target::Command(Command::FocusRight),
        "prefix+l",
    ),
    (
        "swap_pane_left",
        Target::Command(Command::SwapLeft),
        "prefix+shift+h",
    ),
    (
        "swap_pane_down",
        Target::Command(Command::SwapDown),
        "prefix+shift+j",
    ),
    (
        "swap_pane_up",
        Target::Command(Command::SwapUp),
        "prefix+shift+k",
    ),
    (
        "swap_pane_right",
        Target::Command(Command::SwapRight),
        "prefix+shift+l",
    ),
    (
        "cycle_pane_next",
        Target::Command(Command::NextPane),
        "prefix+tab",
    ),
    (
        "cycle_pane_previous",
        Target::Command(Command::PreviousPane),
        "prefix+shift+tab",
    ),
    (
        "split_vertical",
        Target::Command(Command::SplitRight),
        "prefix+v",
    ),
    (
        "split_horizontal",
        Target::Command(Command::SplitDown),
        "prefix+minus",
    ),
    (
        "close_pane",
        Target::Command(Command::ClosePane),
        "prefix+x",
    ),
    ("last_pane", Target::Command(Command::LastPane), ""),
    ("zoom", Target::Command(Command::Zoom), "prefix+z"),
    (
        "resize_mode",
        Target::Command(Command::ResizeMode),
        "prefix+r",
    ),
    ("resize_pane_left", Target::Command(Command::ResizeLeft), ""),
    ("resize_pane_down", Target::Command(Command::ResizeDown), ""),
    ("resize_pane_up", Target::Command(Command::ResizeUp), ""),
    (
        "resize_pane_right",
        Target::Command(Command::ResizeRight),
        "",
    ),
    (
        "toggle_sidebar",
        Target::Command(Command::ToggleSidebar),
        "prefix+b",
    ),
];

/// How a daemon binding is typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// The keystroke alone runs the command.
    Direct(Keystroke),
    /// The prefix, then this keystroke.
    Prefixed(Keystroke),
}

/// The daemon's bindings for GUI commands: those its file sets first, in
/// table order, then Herdr's defaults for the rest. Herdr rejects a default
/// that collides with a binding the user wrote, and the keymap keeps the
/// first binding for a keystroke, so the same one wins here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DaemonKeys {
    /// Every prefix key, never empty and without duplicates. Each one arms
    /// prefix mode; the first is the one shown in chord labels.
    pub(super) prefixes: Vec<Keystroke>,
    pub(super) bindings: Vec<(Command, Trigger)>,
    /// `navigate_workspace_up` and `_down`: the keys that move the workspace
    /// picker's selection, besides its own arrows.
    pub(super) navigate_up: Vec<Keystroke>,
    pub(super) navigate_down: Vec<Keystroke>,
}

impl Default for DaemonKeys {
    /// Herdr's defaults, which also apply when it has no config file.
    fn default() -> Self {
        Self::from_table(None)
    }
}

impl DaemonKeys {
    /// Reads a host's `server_keybindings_toml`: the normalized `[keys]`
    /// profile Herdr publishes for `--remote-keybindings server`. It is read
    /// with the same rules as the local `[keys]` table, so prefixes, actions,
    /// and indexed keys mean the same thing on either side. Like Herdr, a
    /// profile that is missing or unreadable as a whole is an error, which
    /// leaves the caller on its local keybindings.
    pub(crate) fn from_profile(profile: Option<&str>) -> Result<Self> {
        let profile = profile.ok_or(Error::ServerKeybindingsMissing)?;
        if profile.len() > MAX_PROFILE_BYTES {
            return Err(Error::ServerKeybindingsTooLarge {
                max: MAX_PROFILE_BYTES,
            });
        }
        let table: toml::Table = profile.parse().map_err(Error::ServerKeybindingsParse)?;
        let keys = table
            .get("keys")
            .and_then(toml::Value::as_table)
            .ok_or(Error::ServerKeybindingsNoKeys)?;
        Ok(Self::from_table(Some(keys)))
    }

    /// Reads the daemon config's `[keys]` table; each action it leaves out,
    /// or gives a value of the wrong type, keeps Herdr's default.
    pub(crate) fn from_table(keys: Option<&toml::Table>) -> Self {
        let prefixes = prefixes(keys);
        let legacy = |indexed: Indexed| {
            keys.and_then(|keys| keys.get("indexed")?.get(indexed.legacy_key())?.as_str())
                .map(str::trim)
                .filter(|combo| !combo.is_empty() && !combo.starts_with("prefix"))
        };
        let mut configured = Vec::new();
        let mut defaults = Vec::new();
        for &(name, target, default) in ACTIONS {
            let value = keys.and_then(|keys| {
                keys.get(name)
                    // Herdr still accepts zoom's old name.
                    .or_else(|| (name == "zoom").then(|| keys.get("fullscreen")).flatten())
            });
            let written = entries(value);
            // A legacy `[keys.indexed]` combo is user configuration too: it
            // displaces the default unless the action is also set itself.
            let legacy = match target {
                Target::Indexed(indexed) => legacy(indexed),
                Target::Command(_) => None,
            };
            let (list, entries) = if written.is_none() && legacy.is_none() {
                (&mut defaults, vec![default.to_owned()])
            } else {
                let mut entries: Vec<String> = written
                    .unwrap_or_default()
                    .into_iter()
                    .take(MAX_ENTRIES)
                    .map(str::to_owned)
                    .collect();
                entries.extend(legacy.map(|combo| format!("{combo}+1..9")));
                (&mut configured, entries)
            };
            for entry in &entries {
                for (digit, trigger) in triggers(entry) {
                    let command = match (target, digit) {
                        (Target::Command(command), _) => command,
                        (Target::Indexed(indexed), Some(digit)) => indexed.command(digit),
                        (Target::Indexed(_), None) => continue,
                    };
                    list.push((command, trigger));
                }
            }
        }
        configured.append(&mut defaults);
        let navigate = |name: &str, default: &'static str| -> Vec<Keystroke> {
            let entries = entries(keys.and_then(|keys| keys.get(name)));
            // Herdr refuses prefix chords in navigate mode.
            entries
                .unwrap_or_else(|| vec![default])
                .into_iter()
                .take(MAX_ENTRIES)
                .map(str::trim)
                .filter(|entry| !entry.starts_with("prefix+"))
                .filter_map(keystroke)
                .collect()
        };
        Self {
            prefixes,
            bindings: configured,
            navigate_up: navigate("navigate_workspace_up", "up"),
            navigate_down: navigate("navigate_workspace_down", "down"),
        }
    }
}

/// A `[keys]` value's entries, when it has a binding's shape: a string, or a
/// list of strings. An empty one still counts, as Herdr reads it as unbound;
/// any other value leaves the default.
fn entries(value: Option<&toml::Value>) -> Option<Vec<&str>> {
    match value? {
        toml::Value::String(entry) => Some(vec![entry.as_str()]),
        toml::Value::Array(entries) if entries.iter().all(toml::Value::is_str) => {
            Some(entries.iter().filter_map(toml::Value::as_str).collect())
        }
        _ => None,
    }
}

/// `prefix` as Herdr reads it: one string or a list, with the published
/// profiles' `extra_prefixes` appended. Entries this client cannot parse are
/// dropped, later duplicates are ignored, and when nothing usable remains
/// (an empty list included) Herdr's default applies.
fn prefixes(keys: Option<&toml::Table>) -> Vec<Keystroke> {
    let field = |name: &str| entries(keys.and_then(|keys| keys.get(name))).unwrap_or_default();
    let mut prefixes: Vec<Keystroke> = Vec::new();
    let entries = field("prefix").into_iter().chain(field("extra_prefixes"));
    for parsed in entries.take(MAX_ENTRIES).filter_map(keystroke) {
        if !prefixes.contains(&parsed) {
            prefixes.push(parsed);
        }
    }
    if prefixes.is_empty() {
        prefixes.extend(keystroke(DEFAULT_PREFIX));
    }
    prefixes
}

/// Each binding one entry spells, with the digit it types, if any. `1..9`
/// stands for the nine digit keys; an unparseable entry yields nothing.
pub(super) fn triggers(entry: &str) -> Vec<(Option<u8>, Trigger)> {
    let entry = entry.trim();
    let (prefixed, body) = match entry.strip_prefix("prefix+") {
        Some(body) => (true, body),
        None => (false, entry),
    };
    let trigger = |keystroke| {
        if prefixed {
            Trigger::Prefixed(keystroke)
        } else {
            Trigger::Direct(keystroke)
        }
    };
    if body.split('+').any(|part| part.trim() == "1..9") {
        return (1..=9)
            .filter_map(|digit| {
                let keystroke = keystroke(&body.replace("1..9", &digit.to_string()))?;
                Some((Some(digit), trigger(keystroke)))
            })
            .collect();
    }
    let Some(keystroke) = keystroke(body) else {
        return Vec::new();
    };
    let digit = keystroke
        .key
        .parse::<u8>()
        .ok()
        .filter(|digit| (1..=9).contains(digit));
    vec![(digit, trigger(keystroke))]
}

/// One daemon keystroke, `+`-separated as Herdr writes it, in GPUI's terms.
/// Herdr's `hyper` modifier has no GPUI equivalent, so it matches nothing.
fn keystroke(text: &str) -> Option<Keystroke> {
    let mut modifiers = Modifiers::default();
    let mut key = None;
    for part in text.split('+') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.control = true,
            "shift" => modifiers.shift = true,
            "alt" | "option" | "meta" => modifiers.alt = true,
            "cmd" | "command" | "super" => modifiers.platform = true,
            "hyper" => return None,
            _ if key.is_none() => key = Some(part),
            _ => return None,
        }
    }
    let key = key?;
    let named = match key.to_ascii_lowercase().as_str() {
        "space" => "space",
        "enter" | "return" => "enter",
        "esc" | "escape" => "escape",
        "tab" => "tab",
        "backspace" | "bs" => "backspace",
        "left" => "left",
        "right" => "right",
        "up" => "up",
        "down" => "down",
        "minus" => "-",
        "comma" => ",",
        "period" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "quote" => "'",
        "double_quote" | "double-quote" => "\"",
        "semicolon" => ";",
        "colon" => ":",
        "percent" => "%",
        "ampersand" => "&",
        "backtick" => "`",
        "plus" => "+",
        _ => "",
    };
    let mut chars = key.chars();
    let key = match (chars.next(), chars.next()) {
        _ if !named.is_empty() => named.to_owned(),
        (Some(single), None) if single.is_ascii_uppercase() => {
            modifiers.shift = true;
            single.to_ascii_lowercase().to_string()
        }
        (Some(single), None) => single.to_string(),
        _ => {
            let lower = key.to_ascii_lowercase();
            let number: u8 = lower.strip_prefix('f')?.parse().ok()?;
            if !(1..=35).contains(&number) {
                return None;
            }
            lower
        }
    };
    Some(Keystroke {
        modifiers,
        key,
        key_char: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn keys(text: &str) -> DaemonKeys {
        let table: toml::Table = text.parse().unwrap();
        DaemonKeys::from_table(table.get("keys").and_then(toml::Value::as_table))
    }

    fn parsed(text: &str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    fn prefixed(key: &str) -> Trigger {
        Trigger::Prefixed(parsed(key))
    }

    fn bound(keys: &DaemonKeys, command: Command) -> Vec<Trigger> {
        keys.bindings
            .iter()
            .filter(|(bound, _)| *bound == command)
            .map(|(_, trigger)| trigger.clone())
            .collect()
    }

    #[test]
    fn keystrokes_translate_herdr_spelling() {
        for (herdr, gpui) in [
            ("ctrl+b", "ctrl-b"),
            ("Control+Shift+N", "ctrl-shift-n"),
            ("N", "shift-n"),
            ("option+1", "alt-1"),
            ("meta+x", "alt-x"),
            ("cmd+t", "cmd-t"),
            ("super+t", "cmd-t"),
            ("minus", "-"),
            ("\\", "\\"),
            ("backslash", "\\"),
            ("?", "?"),
            ("[", "["),
            ("shift+tab", "shift-tab"),
            ("esc", "escape"),
            ("return", "enter"),
            ("space", "space"),
            ("f12", "f12"),
            ("f", "f"),
        ] {
            assert_eq!(keystroke(herdr), Some(parsed(gpui)), "{herdr}");
        }
        for invalid in [
            "", "ctrl+", "ctrl++b", "a+b", "hyper+x", "f0", "f99", "nope",
        ] {
            assert_eq!(keystroke(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn defaults_match_herdr() {
        let keys = DaemonKeys::default();
        assert_eq!(keys.prefixes, [parsed("ctrl-b")]);
        assert_eq!(bound(&keys, Command::SplitRight), [prefixed("v")]);
        assert_eq!(bound(&keys, Command::SplitDown), [prefixed("-")]);
        assert_eq!(bound(&keys, Command::Keybinds), [prefixed("?")]);
        assert_eq!(bound(&keys, Command::Workspace), [prefixed("shift-n")]);
        assert_eq!(
            bound(&keys, Command::WorkspacePicker),
            [prefixed("w"), prefixed("g")]
        );
        assert_eq!(bound(&keys, Command::PreviousPane), [prefixed("shift-tab")]);
        for digit in 1..=9 {
            let key = prefixed(&digit.to_string());
            assert_eq!(bound(&keys, Command::TabNumber(digit)), [key]);
        }
        assert!(bound(&keys, Command::ClearPane).is_empty());
        assert_eq!(keys, DaemonKeys::from_table(Some(&toml::Table::new())));
    }

    #[test]
    fn the_issue_example_reads_as_written() {
        let keys = keys(
            r#"
            [keys]
            prefix = "ctrl+a"
            split_vertical = ["prefix+v", "prefix+\\"]
            focus_pane_left = ["prefix+h", "prefix+left"]
            switch_tab = ["prefix+1..9", "alt+1..9"]
            "#,
        );
        assert_eq!(keys.prefixes, [parsed("ctrl-a")]);
        assert_eq!(
            bound(&keys, Command::SplitRight),
            [
                Trigger::Prefixed(parsed("v")),
                Trigger::Prefixed(parsed("\\"))
            ]
        );
        assert_eq!(
            bound(&keys, Command::FocusLeft),
            [
                Trigger::Prefixed(parsed("h")),
                Trigger::Prefixed(parsed("left"))
            ]
        );
        assert_eq!(
            bound(&keys, Command::TabNumber(3)),
            [
                Trigger::Prefixed(parsed("3")),
                Trigger::Direct(parsed("alt-3"))
            ]
        );
        // Actions the file leaves alone keep Herdr's defaults.
        assert_eq!(bound(&keys, Command::Tab), [Trigger::Prefixed(parsed("c"))]);
    }

    #[test]
    fn a_server_profile_reads_like_the_local_table() {
        let profile = r#"
            [keys]
            prefix = "ctrl+a"
            extra_prefixes = ["ctrl+s"]
            split_vertical = ["prefix+v", "prefix+\\"]
            switch_tab = "alt+1..9"
            "#;
        let keys = DaemonKeys::from_profile(Some(profile)).unwrap();
        assert_eq!(keys, self::keys(profile));
        // Herdr publishes every prefix after the first as `extra_prefixes`.
        assert_eq!(keys.prefixes, [parsed("ctrl-a"), parsed("ctrl-s")]);
        assert_eq!(
            bound(&keys, Command::TabNumber(2)),
            [Trigger::Direct(parsed("alt-2"))]
        );
        // An empty published table is Herdr's defaults, not a failure.
        assert_eq!(
            DaemonKeys::from_profile(Some("[keys]\n")).unwrap(),
            DaemonKeys::default()
        );
    }

    #[test]
    fn an_unusable_server_profile_is_an_error() {
        assert!(matches!(
            DaemonKeys::from_profile(None),
            Err(Error::ServerKeybindingsMissing)
        ));
        assert!(matches!(
            DaemonKeys::from_profile(Some("[keys\n")),
            Err(Error::ServerKeybindingsParse(_))
        ));
        for profile in ["", "prefix = 'ctrl+a'", "keys = 'ctrl+a'"] {
            assert!(
                matches!(
                    DaemonKeys::from_profile(Some(profile)),
                    Err(Error::ServerKeybindingsNoKeys)
                ),
                "{profile:?}"
            );
        }
        let oversized = format!("[keys]\n{}", "# pad\n".repeat(MAX_PROFILE_BYTES / 6 + 1));
        assert!(matches!(
            DaemonKeys::from_profile(Some(&oversized)),
            Err(Error::ServerKeybindingsTooLarge {
                max: MAX_PROFILE_BYTES
            })
        ));
        let error = DaemonKeys::from_profile(Some("[keys\n")).unwrap_err();
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn unusable_entries_are_skipped_not_fatal() {
        let keys = keys(
            r#"
            [keys]
            prefix = "hyper+a"
            new_tab = ""
            next_tab = ["prefix+hyper+n", "prefix+n"]
            close_tab = 5
            fullscreen = "prefix+f"
            switch_tab = ["prefix+0", "prefix+4", "prefix+x"]
            reload_config = "prefix+r"
            "#,
        );
        // A prefix this client cannot express falls back to ctrl+b.
        assert_eq!(keys.prefixes, [parsed("ctrl-b")]);
        assert!(bound(&keys, Command::Tab).is_empty());
        assert_eq!(
            bound(&keys, Command::NextTab),
            [Trigger::Prefixed(parsed("n"))]
        );
        assert_eq!(
            bound(&keys, Command::CloseTab),
            [Trigger::Prefixed(parsed("shift-x"))]
        );
        assert_eq!(
            bound(&keys, Command::Zoom),
            [Trigger::Prefixed(parsed("f"))]
        );
        assert_eq!(
            bound(&keys, Command::TabNumber(4)),
            [Trigger::Prefixed(parsed("4"))]
        );
        assert!(bound(&keys, Command::TabNumber(1)).is_empty());
    }

    #[test]
    fn layout_and_navigation_actions_take_herdr_defaults() {
        let keys = DaemonKeys::default();
        for (command, key) in [
            (Command::RenameWorkspace, "shift-w"),
            (Command::CloseWorkspace, "shift-d"),
            (Command::ReloadConfig, "shift-r"),
            (Command::RenameTab, "shift-t"),
            (Command::RenamePane, "shift-p"),
            (Command::SwapLeft, "shift-h"),
            (Command::SwapDown, "shift-j"),
            (Command::SwapUp, "shift-k"),
            (Command::SwapRight, "shift-l"),
            (Command::ResizeMode, "r"),
            (Command::EditScrollback, "e"),
            (Command::CopyMode, "["),
        ] {
            assert_eq!(bound(&keys, command), [prefixed(key)], "{command:?}");
        }
        // Herdr leaves these unset until the user binds them.
        for command in [
            Command::MoveTabPrevious,
            Command::MoveTabNext,
            Command::ResizeLeft,
            Command::ResizeRight,
            Command::ResizeUp,
            Command::ResizeDown,
            Command::LastPane,
            Command::PreviousWorkspace,
            Command::NextWorkspace,
            Command::WorkspaceNumber(1),
            Command::PreviousAgent,
            Command::NextAgent,
            Command::AgentNumber(1),
        ] {
            assert!(bound(&keys, command).is_empty(), "{command:?}");
        }
        assert_eq!(keys.navigate_up, [parsed("up")]);
        assert_eq!(keys.navigate_down, [parsed("down")]);
    }

    #[test]
    fn configured_actions_bind_and_indexed_actions_count_digits() {
        let keys = keys(
            r#"
            [keys]
            move_tab_previous = "alt+shift+["
            move_tab_next = "alt+shift+]"
            resize_pane_left = ["prefix+alt+h", "ctrl+alt+left"]
            last_pane = "prefix+;"
            previous_workspace = "ctrl+alt+k"
            next_agent = "prefix+a"
            switch_workspace = "prefix+alt+1..9"
            focus_agent = ["ctrl+1", "ctrl+2"]
            navigate_workspace_up = ["k", "ctrl+p", "prefix+p"]
            "#,
        );
        assert_eq!(
            bound(&keys, Command::MoveTabPrevious),
            [Trigger::Direct(parsed("alt-shift-["))]
        );
        assert_eq!(
            bound(&keys, Command::MoveTabNext),
            [Trigger::Direct(parsed("alt-shift-]"))]
        );
        assert_eq!(
            bound(&keys, Command::ResizeLeft),
            [prefixed("alt-h"), Trigger::Direct(parsed("ctrl-alt-left"))]
        );
        assert_eq!(bound(&keys, Command::LastPane), [prefixed(";")]);
        assert_eq!(
            bound(&keys, Command::PreviousWorkspace),
            [Trigger::Direct(parsed("ctrl-alt-k"))]
        );
        assert_eq!(bound(&keys, Command::NextAgent), [prefixed("a")]);
        for digit in 1..=9 {
            let key = prefixed(&format!("alt-{digit}"));
            assert_eq!(bound(&keys, Command::WorkspaceNumber(digit)), [key]);
        }
        assert_eq!(
            bound(&keys, Command::AgentNumber(2)),
            [Trigger::Direct(parsed("ctrl-2"))]
        );
        assert!(bound(&keys, Command::AgentNumber(3)).is_empty());
        // Navigate mode takes no prefix chords.
        assert_eq!(keys.navigate_up, [parsed("k"), parsed("ctrl-p")]);
        assert_eq!(keys.navigate_down, [parsed("down")]);
    }

    /// `[keys.indexed]` names a modifier combo for the digits 1-9. Like
    /// Herdr, it displaces the action's default unless the action is set too.
    #[test]
    fn legacy_indexed_combos_bind_digits() {
        let keys = keys(
            r#"
            [keys]
            switch_workspace = "prefix+alt+1..9"
            [keys.indexed]
            tabs = "alt"
            workspaces = "ctrl+shift"
            agents = "  "
            "#,
        );
        for digit in 1..=9 {
            let key = |text: String| Trigger::Direct(parsed(&text));
            assert_eq!(
                bound(&keys, Command::TabNumber(digit)),
                [key(format!("alt-{digit}"))]
            );
            assert_eq!(
                bound(&keys, Command::WorkspaceNumber(digit)),
                [
                    prefixed(&format!("alt-{digit}")),
                    key(format!("ctrl-shift-{digit}"))
                ]
            );
            assert!(bound(&keys, Command::AgentNumber(digit)).is_empty());
        }
        // A combo Herdr cannot read binds nothing, yet still displaces.
        let hyper = self::keys("[keys.indexed]\ntabs = \"hyper\"");
        assert!(bound(&hyper, Command::TabNumber(1)).is_empty());
    }

    /// Herdr drops a default that collides with a binding the user wrote, so
    /// user bindings come first and win the keymap's first-come keystroke.
    #[test]
    fn user_bindings_precede_defaults() {
        let keys = keys(
            r#"
            [keys]
            reload_config = "prefix+r"
            "#,
        );
        let first = keys
            .bindings
            .iter()
            .position(|(_, trigger)| *trigger == prefixed("r"))
            .unwrap();
        assert_eq!(keys.bindings[first].0, Command::ReloadConfig);
        assert_eq!(bound(&keys, Command::ResizeMode), [prefixed("r")]);
        let keymap = super::super::Keymap::with_overrides(&Default::default(), &keys).unwrap();
        assert_eq!(keymap.chord(&parsed("r")), Some(Command::ReloadConfig));
    }

    /// Herdr leaves the worktree and detach actions unbound, so they bind
    /// only what the user writes, prefix chords and direct keys alike.
    #[test]
    fn worktree_and_detach_actions_bind_what_the_user_writes() {
        let defaults = DaemonKeys::default();
        for command in [
            Command::OpenWorktree,
            Command::RemoveWorktree,
            Command::Detach,
        ] {
            assert!(bound(&defaults, command).is_empty(), "{command:?}");
        }
        let keys = keys(
            r#"
            [keys]
            open_worktree = "prefix+shift+o"
            remove_worktree = ["prefix+hyper+x", "prefix+alt+x"]
            detach = "prefix+d"
            "#,
        );
        assert_eq!(bound(&keys, Command::OpenWorktree), [prefixed("shift-o")]);
        // `hyper` has no GPUI equivalent, so only the second entry binds.
        assert_eq!(bound(&keys, Command::RemoveWorktree), [prefixed("alt-x")]);
        assert_eq!(bound(&keys, Command::Detach), [prefixed("d")]);
        let keymap = super::super::Keymap::with_overrides(&Default::default(), &keys).unwrap();
        assert_eq!(keymap.chord(&parsed("d")), Some(Command::Detach));
        assert_eq!(
            keymap.chord(&parsed("shift-o")),
            Some(Command::OpenWorktree)
        );
        assert_eq!(
            keymap.chord(&parsed("alt-x")),
            Some(Command::RemoveWorktree)
        );
        let direct = self::keys("[keys]\ndetach = \"ctrl+alt+q\"");
        assert_eq!(
            bound(&direct, Command::Detach),
            [Trigger::Direct(parsed("ctrl-alt-q"))]
        );
        // Navigate mode's pane keys stay unmapped and bind nothing.
        let navigate = self::keys("[keys]\nnavigate_pane_left = \"h\"");
        assert_eq!(navigate, DaemonKeys::default());
    }

    #[test]
    fn a_prefix_list_keeps_every_key_in_order() {
        let keys = keys(
            r#"
            [keys]
            prefix = ["ctrl+space", "ctrl+s"]
            "#,
        );
        assert_eq!(keys.prefixes, [parsed("ctrl-space"), parsed("ctrl-s")]);
        // Prefixed actions do not depend on which prefix armed them.
        assert_eq!(bound(&keys, Command::Tab), [prefixed("c")]);
    }

    #[test]
    fn prefix_lists_follow_herdr_validation() {
        let prefixes = |value: &str| keys(&format!("[keys]\nprefix = {value}")).prefixes;
        // Invalid entries are dropped while valid ones survive.
        assert_eq!(prefixes(r#"["ctrl+a", "wat", ""]"#), [parsed("ctrl-a")]);
        // Later duplicates, however spelled, are ignored.
        assert_eq!(
            prefixes(r#"["ctrl+a", " Control+a ", "f12", "ctrl+a"]"#),
            [parsed("ctrl-a"), parsed("f12")]
        );
        // An empty list, or one with nothing usable, keeps the default.
        assert_eq!(prefixes("[]"), [parsed("ctrl-b")]);
        assert_eq!(prefixes(r#"["wat", "hyper+a"]"#), [parsed("ctrl-b")]);
        // Herdr rejects a list of anything but strings, as does a number.
        assert_eq!(prefixes(r#"["ctrl+a", 5]"#), [parsed("ctrl-b")]);
        assert_eq!(prefixes("5"), [parsed("ctrl-b")]);
    }

    #[test]
    fn published_extra_prefixes_follow_the_primary() {
        let keys = keys(
            r#"
            [keys]
            prefix = "ctrl+a"
            extra_prefixes = ["ctrl+s", "ctrl+a"]
            "#,
        );
        assert_eq!(keys.prefixes, [parsed("ctrl-a"), parsed("ctrl-s")]);
    }
}

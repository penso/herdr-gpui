//! The keystrokes bound to each catalog command: the defaults in
//! `controls::COMMANDS`, then the daemon config's `[keys]` table (prefix
//! chords included), with the GUI config file's `[keybindings]` table layered
//! on top. The palette, keybindings page, menu bar, GPUI keymap, and prefix
//! mode all read this one resolved answer. The daemon's `[[keys.command]]`
//! shortcuts arrive with each snapshot instead, so they are matched against
//! this answer when typed rather than resolved into it.

mod daemon;

pub(crate) use daemon::DaemonKeys;

use crate::{
    Error, Result,
    controls::{COMMANDS, Command},
};
use daemon::Trigger;
use gpui::{KeybindingKeystroke, Keystroke, Modifiers};
use herdr_client::protocol::ClientShellCommand;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A command may carry an alias or two, never an unbounded list.
const MAX_KEYSTROKES: usize = 8;

/// Far more chords than a keyboard has spare.
const MAX_PANE_KEYS: usize = 64;

/// The config's `[pane_keys]` table: a keystroke, and the keystroke the
/// focused pane receives for it instead. An empty value removes a default.
pub(crate) type PaneKeys = BTreeMap<String, String>;

/// Ghostty's macOS line-editing chords, which every other Mac terminal also
/// sends: Cmd-Left and Cmd-Right to the line's start and end, Cmd-Backspace
/// to delete back to its start. Elsewhere the platform key is the desktop's.
const DEFAULT_PANE_KEYS: &[(&str, &str)] = &[
    ("cmd-left", "ctrl-a"),
    ("cmd-right", "ctrl-e"),
    ("cmd-backspace", "ctrl-u"),
];

/// Keystrokes the GUI config binds, with the command holding each. Spelling
/// differences still name the same keystroke.
type Claimed = HashMap<(Modifiers, String), &'static str>;

fn identity(keystroke: &Keystroke) -> (Modifiers, String) {
    (keystroke.modifiers, keystroke.key.clone())
}

/// One entry of the config's `[keybindings]` table: a keystroke, or a list of
/// them. An empty string or list leaves the command unbound.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub(crate) enum Binding {
    One(String),
    Many(Vec<String>),
}

impl Binding {
    fn keystrokes(&self) -> impl Iterator<Item = &str> {
        let keystrokes = match self {
            Self::One(keystroke) => std::slice::from_ref(keystroke),
            Self::Many(keystrokes) => keystrokes.as_slice(),
        };
        keystrokes
            .iter()
            .map(|keystroke| keystroke.trim())
            .filter(|keystroke| !keystroke.is_empty())
    }
}

/// One way to run a command.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shortcut {
    /// As shown to the user: a GPUI keystroke, or the prefix and the key
    /// typed after it, separated by a space.
    label: String,
    /// The key that completes a prefix chord. `None` binds `label` directly
    /// through GPUI's keymap.
    chord: Option<Keystroke>,
}

impl Shortcut {
    fn direct(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            chord: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    /// Parallel to `COMMANDS`, primary shortcut first.
    shortcuts: Vec<Vec<Shortcut>>,
    /// The keystrokes that start a chord, less any nothing can use. The
    /// first one is the prefix shown to the user.
    prefixes: Vec<Keystroke>,
    /// Keys that move the workspace picker's selection up and down.
    navigate_up: Vec<Keystroke>,
    navigate_down: Vec<Keystroke>,
    /// Keystrokes the focused pane receives as another keystroke. No command
    /// is bound to them, so GPUI's keymap never claims them first.
    pane_keys: Vec<(Keystroke, Keystroke)>,
}

impl Default for Keymap {
    /// The catalog under Herdr's own default `[keys]`, as when neither config
    /// file names a keystroke.
    fn default() -> Self {
        let pane_keys: Vec<_> = default_pane_keys().collect();
        let claimed = pane_keys
            .iter()
            .map(|(typed, _)| (identity(typed), "pane_keys"))
            .collect();
        Self::layer(
            vec![None; COMMANDS.len()],
            &claimed,
            &DaemonKeys::default(),
            pane_keys,
        )
    }
}

impl Keymap {
    /// Layers the daemon's `keys` and then `overrides` over the defaults. A
    /// keystroke the GUI config assigns moves to that command, so rebinding
    /// one key never requires unbinding its default or daemon owner too; two
    /// configured commands claiming it is an error. A command the GUI config
    /// names keeps exactly the keystrokes listed there. `pane_keys` take
    /// their keystrokes from every default and daemon command, but not from
    /// one the GUI config names.
    pub(crate) fn with_overrides(
        overrides: &BTreeMap<String, Binding>,
        pane_keys: &PaneKeys,
        keys: &DaemonKeys,
    ) -> Result<Self> {
        let mut configured = vec![None; COMMANDS.len()];
        for (name, binding) in overrides {
            let index = COMMANDS
                .iter()
                .position(|info| info.name == name)
                .ok_or_else(|| Error::UnknownKeybinding(name.clone()))?;
            let command = COMMANDS[index].name;
            let keystrokes: Vec<String> = binding.keystrokes().map(str::to_owned).collect();
            if keystrokes.len() > MAX_KEYSTROKES {
                return Err(Error::TooManyKeystrokes(command));
            }
            configured[index] = Some(keystrokes);
        }
        let mut claimed = Claimed::new();
        for (info, keystrokes) in COMMANDS.iter().zip(&configured) {
            for keystroke in keystrokes.iter().flatten() {
                let parsed = parse(info.name, keystroke)?;
                if let Some(first) = claimed.insert(identity(&parsed), info.name)
                    && first != info.name
                {
                    return Err(Error::DuplicateKeystroke {
                        keystroke: keystroke.clone(),
                        first,
                        second: info.name,
                    });
                }
            }
        }
        let pane_keys = resolve_pane_keys(pane_keys, default_pane_keys())?;
        for (typed, _) in &pane_keys {
            if let Some(command) = claimed.insert(identity(typed), "pane_keys") {
                return Err(Error::PaneKeyBound {
                    keystroke: typed.unparse(),
                    command,
                });
            }
        }
        Ok(Self::layer(configured, &claimed, keys, pane_keys))
    }

    /// Herdr owns and validates its own file, so a daemon binding this client
    /// cannot honor, or that collides with one already placed, is skipped
    /// rather than reported: the first daemon binding for a keystroke wins,
    /// as does any GUI-configured keystroke over the daemon's.
    fn layer(
        configured: Vec<Option<Vec<String>>>,
        claimed: &Claimed,
        keys: &DaemonKeys,
        pane_keys: Vec<(Keystroke, Keystroke)>,
    ) -> Self {
        // Keystrokes bound directly so far, which later layers cannot take.
        let mut taken: HashSet<_> = claimed.keys().cloned().collect();
        let prefixes: Vec<Keystroke> = keys
            .prefixes
            .iter()
            .filter(|prefix| usable_prefix(prefix) && taken.insert(identity(prefix)))
            .cloned()
            .collect();
        let mut chords = HashSet::new();
        let mut from_daemon = vec![Vec::new(); COMMANDS.len()];
        for (command, trigger) in &keys.bindings {
            let Some(index) = COMMANDS.iter().position(|info| info.command == *command) else {
                continue;
            };
            if configured[index].is_some() {
                continue;
            }
            let shortcut = match trigger {
                Trigger::Direct(keystroke) => {
                    if !has_modifier(keystroke) || !taken.insert(identity(keystroke)) {
                        continue;
                    }
                    Shortcut::direct(keystroke.unparse())
                }
                Trigger::Prefixed(keystroke) => {
                    // A prefix typed after a prefix sends it to the terminal
                    // instead.
                    let Some(prefix) = prefixes.first() else {
                        continue;
                    };
                    if prefixes
                        .iter()
                        .any(|prefix| identity(keystroke) == identity(prefix))
                        || !chords.insert(identity(keystroke))
                    {
                        continue;
                    }
                    Shortcut {
                        label: format!("{} {}", prefix.unparse(), keystroke.unparse()),
                        chord: Some(keystroke.clone()),
                    }
                }
            };
            from_daemon[index].push(shortcut);
        }
        let shortcuts = COMMANDS
            .iter()
            .zip(configured)
            .zip(from_daemon)
            .map(|((info, configured), from_daemon)| match configured {
                Some(keystrokes) => keystrokes.into_iter().map(Shortcut::direct).collect(),
                None => info
                    .shortcuts
                    .iter()
                    .filter(|shortcut| {
                        Keystroke::parse(shortcut)
                            .is_ok_and(|parsed| !taken.contains(&identity(&parsed)))
                    })
                    .map(|&shortcut| Shortcut::direct(shortcut))
                    .chain(from_daemon)
                    .collect(),
            })
            .collect();
        Self {
            shortcuts,
            prefixes,
            navigate_up: keys.navigate_up.clone(),
            navigate_down: keys.navigate_down.clone(),
            pane_keys,
        }
    }

    /// The keystroke the focused pane receives when `typed` is pressed.
    pub(crate) fn pane_key(&self, typed: &Keystroke) -> Option<&Keystroke> {
        self.pane_keys
            .iter()
            .find(|(bound, _)| typed_matches(typed, bound))
            .map(|(_, sent)| sent)
    }

    /// Every shortcut bound to `command`, primary first. A prefix chord reads
    /// as the prefix and the key after it, separated by a space.
    pub fn shortcuts(&self, command: Command) -> impl Iterator<Item = &str> {
        COMMANDS
            .iter()
            .position(|info| info.command == command)
            .and_then(|index| self.shortcuts.get(index))
            .into_iter()
            .flatten()
            .map(|shortcut| shortcut.label.as_str())
    }

    /// The shortcut shown beside `command`, or `""` when it is unbound.
    pub fn primary(&self, command: Command) -> &str {
        self.shortcuts(command).next().unwrap_or("")
    }

    /// Each keystroke GPUI binds directly, with the command it runs, in
    /// catalog order. Prefix chords are not among them; see `chord`.
    pub fn bindings(&self) -> impl Iterator<Item = (Command, &str)> {
        COMMANDS
            .iter()
            .zip(&self.shortcuts)
            .flat_map(|(info, shortcuts)| {
                shortcuts
                    .iter()
                    .filter(|shortcut| shortcut.chord.is_none())
                    .map(move |shortcut| (info.command, shortcut.label.as_str()))
            })
    }

    /// The first prefix as shown to the user, while chords can use one.
    pub(crate) fn prefix_label(&self) -> Option<String> {
        self.prefixes.first().map(Keystroke::unparse)
    }

    /// Whether `typed` is one of the keystrokes that start a chord.
    pub(crate) fn is_prefix(&self, typed: &Keystroke) -> bool {
        self.prefixes
            .iter()
            .any(|prefix| typed_matches(typed, prefix))
    }

    /// The command a chord runs when `typed` follows the prefix.
    pub(crate) fn chord(&self, typed: &Keystroke) -> Option<Command> {
        COMMANDS
            .iter()
            .zip(&self.shortcuts)
            .find(|(_, shortcuts)| {
                shortcuts.iter().any(|shortcut| {
                    shortcut
                        .chord
                        .as_ref()
                        .is_some_and(|chord| typed_matches(typed, chord))
                })
            })
            .map(|(info, _)| info.command)
    }

    /// Whether `typed`, alone or after the prefix as `prefixed` says, is one
    /// of `command`'s shortcuts.
    pub(crate) fn triggers(&self, command: Command, typed: &Keystroke, prefixed: bool) -> bool {
        COMMANDS
            .iter()
            .position(|info| info.command == command)
            .and_then(|index| self.shortcuts.get(index))
            .into_iter()
            .flatten()
            .any(|shortcut| match (&shortcut.chord, prefixed) {
                (Some(chord), true) => typed_matches(typed, chord),
                (None, false) => Keystroke::parse(&shortcut.label)
                    .is_ok_and(|bound| typed_matches(typed, &bound)),
                _ => false,
            })
    }

    /// Whether `typed` moves the workspace picker's selection: `Some(true)`
    /// up, `Some(false)` down. A bare character would be typing into its
    /// search field instead, and the picker keeps Escape, Enter, and Tab, so
    /// only other keys that cannot be text count.
    pub(crate) fn navigates_workspace(&self, typed: &Keystroke) -> Option<bool> {
        let text_safe = |bound: &&Keystroke| {
            has_modifier(bound)
                || (bound.key.chars().nth(1).is_some()
                    && !matches!(bound.key.as_str(), "escape" | "enter" | "tab"))
        };
        let matches = |keys: &[Keystroke]| {
            keys.iter()
                .filter(text_safe)
                .any(|bound| typed_matches(typed, bound))
        };
        if matches(&self.navigate_up) {
            Some(true)
        } else if matches(&self.navigate_down) {
            Some(false)
        } else {
            None
        }
    }

    /// The daemon custom command `typed` runs, alone or after the prefix as
    /// `prefixed` says.
    pub(crate) fn custom_command<'a>(
        &self,
        commands: &'a [ClientShellCommand],
        typed: &Keystroke,
        prefixed: bool,
    ) -> Option<&'a ClientShellCommand> {
        // Matching first keeps ordinary typing from checking every trigger
        // against the whole keymap.
        commands.iter().find(|command| {
            custom_triggers(command)
                .filter(|trigger| match trigger {
                    Trigger::Prefixed(bound) => prefixed && typed_matches(typed, bound),
                    Trigger::Direct(bound) => !prefixed && typed_matches(typed, bound),
                })
                .any(|trigger| self.runs_custom(&trigger))
        })
    }

    /// A daemon custom command's shortcuts as this keymap shows them.
    pub(crate) fn custom_labels(&self, command: &ClientShellCommand) -> Vec<String> {
        let mut labels: Vec<String> = custom_triggers(command)
            .filter(|trigger| self.runs_custom(trigger))
            .filter_map(|trigger| match trigger {
                Trigger::Direct(bound) => Some(bound.unparse()),
                Trigger::Prefixed(bound) => self
                    .prefixes
                    .first()
                    .map(|prefix| format!("{} {}", prefix.unparse(), bound.unparse())),
            })
            .collect();
        labels.dedup();
        labels
    }

    /// Whether a custom command's trigger can run it here. Herdr resolves
    /// its own actions before custom commands, so a keystroke this keymap
    /// already binds, or the prefix itself, never reaches one, and a direct
    /// keystroke needs a modifier so typing still reaches the terminal.
    fn runs_custom(&self, trigger: &Trigger) -> bool {
        match trigger {
            // Any prefix typed after a prefix passes it through instead.
            Trigger::Prefixed(bound) => {
                !self.prefixes.is_empty() && !self.is_prefix(bound) && self.chord(bound).is_none()
            }
            Trigger::Direct(bound) => {
                has_modifier(bound)
                    && !self.is_prefix(bound)
                    && self.pane_key(bound).is_none()
                    && !self.bindings().any(|(_, label)| {
                        Keystroke::parse(label)
                            .is_ok_and(|label| identity(&label) == identity(bound))
                    })
            }
        }
    }
}

/// The triggers a custom command's daemon labels spell, as Herdr writes
/// them (`prefix+g`, `ctrl+alt+g`). `binding_label` is only for display: it
/// drops the `prefix+` that tells a chord from a direct keystroke.
fn custom_triggers(command: &ClientShellCommand) -> impl Iterator<Item = Trigger> + '_ {
    command
        .binding_labels
        .iter()
        .take(MAX_KEYSTROKES)
        .flat_map(|label| daemon::triggers(label))
        .map(|(_, trigger)| trigger)
}

/// GPUI's own matching, so a shifted symbol such as `?` matches however the
/// keyboard reports it.
fn typed_matches(typed: &Keystroke, bound: &Keystroke) -> bool {
    typed.should_match(&KeybindingKeystroke::from_keystroke(bound.clone()))
}

/// A keystroke without one of these would stop typing from reaching the
/// terminal.
fn has_modifier(keystroke: &Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    modifiers.platform || modifiers.control || modifiers.alt || modifiers.function
}

/// Herdr documents `esc` and function keys as prefixes; any other bare key
/// would swallow ordinary typing.
fn usable_prefix(prefix: &Keystroke) -> bool {
    has_modifier(prefix)
        || prefix.key == "escape"
        || prefix
            .key
            .strip_prefix('f')
            .is_some_and(|number| number.parse::<u8>().is_ok())
}

/// The built-in pane keys, macOS only: Ghostty binds them there alone, and
/// elsewhere the platform key belongs to the desktop.
fn default_pane_keys() -> impl Iterator<Item = (Keystroke, Keystroke)> {
    DEFAULT_PANE_KEYS
        .iter()
        .filter(|_| cfg!(target_os = "macos"))
        .filter_map(|(typed, sent)| {
            Some((Keystroke::parse(typed).ok()?, Keystroke::parse(sent).ok()?))
        })
}

/// Layers the config's `[pane_keys]` over `defaults`. A keystroke spelled
/// differently still replaces its default, and an empty value removes it.
fn resolve_pane_keys(
    table: &PaneKeys,
    defaults: impl IntoIterator<Item = (Keystroke, Keystroke)>,
) -> Result<Vec<(Keystroke, Keystroke)>> {
    if table.len() > MAX_PANE_KEYS {
        return Err(Error::TooManyPaneKeys(MAX_PANE_KEYS));
    }
    let parse = |keystroke: &str| {
        Keystroke::parse(keystroke).map_err(|source| Error::InvalidPaneKey {
            keystroke: keystroke.to_owned(),
            source,
        })
    };
    let mut resolved: Vec<_> = defaults.into_iter().collect();
    let mut seen = HashSet::new();
    for (typed, sent) in table {
        let parsed = parse(typed.trim())?;
        // A bare character or space is typing, which must reach the pane as text.
        if !has_modifier(&parsed) && (parsed.key.chars().nth(1).is_none() || parsed.key == "space")
        {
            return Err(Error::PaneKeyWithoutModifier(typed.clone()));
        }
        if !seen.insert(identity(&parsed)) {
            return Err(Error::DuplicatePaneKey(typed.clone()));
        }
        resolved.retain(|(bound, _)| identity(bound) != identity(&parsed));
        let sent = sent.trim();
        if sent.is_empty() {
            continue;
        }
        let sent_parsed = parse(sent)?;
        if !crate::terminal::reaches_pane(&sent_parsed) {
            return Err(Error::UnsendablePaneKey {
                from: typed.clone(),
                to: sent.to_owned(),
            });
        }
        resolved.push((parsed, sent_parsed));
    }
    Ok(resolved)
}

fn parse(command: &'static str, keystroke: &str) -> Result<Keystroke> {
    let parsed = Keystroke::parse(keystroke).map_err(|source| Error::InvalidKeystroke {
        command,
        keystroke: keystroke.to_owned(),
        source,
    })?;
    if !has_modifier(&parsed) {
        return Err(Error::KeystrokeWithoutModifier {
            command,
            keystroke: keystroke.to_owned(),
        });
    }
    Ok(parsed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn overrides(entries: &[(&str, Binding)]) -> BTreeMap<String, Binding> {
        entries
            .iter()
            .map(|(name, binding)| ((*name).to_owned(), binding.clone()))
            .collect()
    }

    fn one(keystroke: &str) -> Binding {
        Binding::One(keystroke.into())
    }

    fn keystroke(text: &str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    /// A daemon config that binds nothing, isolating the GUI layers.
    fn no_keys() -> DaemonKeys {
        DaemonKeys {
            prefixes: vec![keystroke("ctrl-b")],
            bindings: Vec::new(),
            navigate_up: Vec::new(),
            navigate_down: Vec::new(),
        }
    }

    fn with(entries: &[(&str, Binding)]) -> Result<Keymap> {
        Keymap::with_overrides(&overrides(entries), &PaneKeys::new(), &no_keys())
    }

    fn list(keymap: &Keymap, command: Command) -> Vec<&str> {
        keymap.shortcuts(command).collect()
    }

    #[test]
    fn defaults_follow_the_catalog_and_herdr() {
        let keymap = Keymap::default();
        assert_eq!(list(&keymap, Command::Tab), ["cmd-t", "ctrl-b c"]);
        assert_eq!(keymap.primary(Command::Tab), "cmd-t");
        assert_eq!(keymap.primary(Command::Workspace), "cmd-shift-n");
        assert_eq!(keymap.primary(Command::Themes), "");
        assert_eq!(
            list(&keymap, Command::SplitDown),
            ["cmd-shift-d", "ctrl-b -"]
        );
        assert_eq!(
            Keymap::with_overrides(
                &BTreeMap::new(),
                &Default::default(),
                &DaemonKeys::default()
            )
            .unwrap(),
            Keymap::default()
        );
        // Herdr's defaults are all chords, so GPUI binds only the catalog.
        let bound: usize = COMMANDS.iter().map(|info| info.shortcuts.len()).sum();
        assert_eq!(keymap.bindings().count(), bound);
        assert!(
            keymap
                .bindings()
                .all(|(_, keystroke)| !keystroke.contains(' '))
        );
    }

    #[test]
    fn overrides_replace_lists_and_empty_values_unbind() {
        let keymap = with(&[
            ("new_workspace", one("cmd-alt-t")),
            (
                "themes",
                Binding::Many(vec!["ctrl-shift-t".into(), " cmd-k ".into()]),
            ),
            ("close_pane", one("")),
            ("toggle_sidebar", Binding::Many(Vec::new())),
        ])
        .unwrap();
        assert_eq!(list(&keymap, Command::Workspace), ["cmd-alt-t"]);
        assert_eq!(list(&keymap, Command::Themes), ["ctrl-shift-t", "cmd-k"]);
        assert!(list(&keymap, Command::ClosePane).is_empty());
        assert!(list(&keymap, Command::ToggleSidebar).is_empty());
        assert_eq!(list(&keymap, Command::Tab), ["cmd-t"]);
    }

    /// Taking a default keystroke must not force the user to also unbind it
    /// from the command that shipped with it.
    #[test]
    fn configured_keystroke_moves_from_its_default_owner() {
        let keymap = with(&[("new_workspace", one("cmd-t"))]).unwrap();
        assert_eq!(list(&keymap, Command::Workspace), ["cmd-t"]);
        assert!(list(&keymap, Command::Tab).is_empty());
        // Spelling differences still name the same keystroke.
        let keymap = with(&[("about", one("CMD-shift-P"))]).unwrap();
        assert!(list(&keymap, Command::Palette).is_empty());
        let keys: Vec<_> = keymap.bindings().map(|(_, keystroke)| keystroke).collect();
        let unique: HashSet<_> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn invalid_configuration_reports_the_command() {
        let error = |entries: &[(&str, Binding)]| with(entries).unwrap_err();
        assert!(matches!(
            error(&[("new_space", one("cmd-n"))]),
            Error::UnknownKeybinding(name) if name == "new_space"
        ));
        let invalid = error(&[("new_tab", one("cmd-n-t"))]);
        assert!(std::error::Error::source(&invalid).is_some());
        assert!(matches!(
            invalid,
            Error::InvalidKeystroke { command: "new_tab", ref keystroke, .. } if keystroke == "cmd-n-t"
        ));
        for keystroke in ["n", "shift-n"] {
            assert!(matches!(
                error(&[("new_tab", one(keystroke))]),
                Error::KeystrokeWithoutModifier {
                    command: "new_tab",
                    ..
                }
            ));
        }
        assert!(matches!(
            error(&[("new_tab", Binding::Many(vec!["cmd-n".into(); 9]))]),
            Error::TooManyKeystrokes("new_tab")
        ));
        assert!(matches!(
            error(&[("new_tab", one("cmd-k")), ("themes", one("cmd-k"))]),
            Error::DuplicateKeystroke {
                first: "new_tab",
                second: "themes",
                ..
            }
        ));
        // Repeating a keystroke within one command is harmless.
        with(&[(
            "new_tab",
            Binding::Many(vec!["cmd-k".into(), "cmd-k".into()]),
        )])
        .unwrap();
    }

    #[test]
    fn chords_follow_the_prefix() {
        let keymap = Keymap::default();
        assert!(keymap.is_prefix(&keystroke("ctrl-b")));
        assert!(!keymap.is_prefix(&keystroke("ctrl-a")));
        assert!(!keymap.is_prefix(&keystroke("b")));
        assert_eq!(keymap.chord(&keystroke("c")), Some(Command::Tab));
        assert_eq!(keymap.chord(&keystroke("v")), Some(Command::SplitRight));
        assert_eq!(
            keymap.chord(&keystroke("shift-n")),
            Some(Command::Workspace)
        );
        assert_eq!(keymap.chord(&keystroke("3")), Some(Command::TabNumber(3)));
        assert_eq!(
            keymap.chord(&keystroke("shift-tab")),
            Some(Command::PreviousPane)
        );
        // A shifted symbol matches however the keyboard reports it.
        assert_eq!(
            keymap.chord(&keystroke("shift-/->?")),
            Some(Command::Keybinds)
        );
        assert_eq!(keymap.chord(&keystroke("?")), Some(Command::Keybinds));
        assert_eq!(keymap.chord(&keystroke("n")), Some(Command::NextTab));
        assert_eq!(keymap.chord(&keystroke("y")), None);
        assert_eq!(keymap.chord(&keystroke("ctrl-c")), None);
    }

    #[test]
    fn gui_overrides_replace_daemon_bindings() {
        let keys = DaemonKeys {
            prefixes: vec![keystroke("ctrl-a")],
            bindings: vec![
                (Command::Tab, Trigger::Prefixed(keystroke("c"))),
                (Command::Tab, Trigger::Direct(keystroke("alt-t"))),
                (Command::SplitRight, Trigger::Prefixed(keystroke("v"))),
            ],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(
            &overrides(&[("new_tab", one("cmd-y"))]),
            &Default::default(),
            &keys,
        )
        .unwrap();
        assert_eq!(list(&keymap, Command::Tab), ["cmd-y"]);
        assert_eq!(keymap.chord(&keystroke("c")), None);
        assert_eq!(list(&keymap, Command::SplitRight), ["cmd-d", "ctrl-a v"]);
        assert_eq!(keymap.chord(&keystroke("v")), Some(Command::SplitRight));
        // An empty GUI entry unbinds the daemon's chords too.
        let keymap = Keymap::with_overrides(
            &overrides(&[("split_right", one(""))]),
            &Default::default(),
            &keys,
        )
        .unwrap();
        assert!(list(&keymap, Command::SplitRight).is_empty());
        assert_eq!(keymap.chord(&keystroke("v")), None);
    }

    #[test]
    fn daemon_keystrokes_move_from_gui_defaults_but_not_from_gui_config() {
        let keys = DaemonKeys {
            prefixes: vec![keystroke("ctrl-a")],
            bindings: vec![
                // cmd-d is Split Right's catalog default.
                (Command::Zoom, Trigger::Direct(keystroke("cmd-d"))),
                // The first daemon binding for a keystroke wins.
                (Command::Tab, Trigger::Direct(keystroke("alt-1"))),
                (Command::TabNumber(1), Trigger::Direct(keystroke("alt-1"))),
                (Command::Tab, Trigger::Prefixed(keystroke("c"))),
                (Command::CloseTab, Trigger::Prefixed(keystroke("c"))),
                // Configured below, so the GUI keeps it.
                (Command::ClosePane, Trigger::Direct(keystroke("cmd-e"))),
                // Would swallow typing.
                (Command::NextTab, Trigger::Direct(keystroke("shift-n"))),
                // The prefix typed twice passes it through instead.
                (Command::PreviousTab, Trigger::Prefixed(keystroke("ctrl-a"))),
            ],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(
            &overrides(&[("about", one("cmd-e"))]),
            &Default::default(),
            &keys,
        )
        .unwrap();
        // Daemon keystrokes read in GPUI's platform spelling (`super-d` on Linux).
        let moved = keystroke("cmd-d").unparse();
        assert_eq!(
            list(&keymap, Command::Zoom),
            ["cmd-shift-enter", moved.as_str()]
        );
        assert!(list(&keymap, Command::SplitRight).is_empty());
        assert_eq!(list(&keymap, Command::Tab), ["cmd-t", "alt-1", "ctrl-a c"]);
        assert_eq!(list(&keymap, Command::TabNumber(1)), ["cmd-1"]);
        assert_eq!(list(&keymap, Command::CloseTab), ["cmd-shift-w"]);
        assert_eq!(list(&keymap, Command::ClosePane), ["cmd-w"]);
        assert_eq!(list(&keymap, Command::About), ["cmd-e"]);
        assert_eq!(list(&keymap, Command::NextTab), ["cmd-shift-]"]);
        assert_eq!(list(&keymap, Command::PreviousTab), ["cmd-shift-["]);
        let keys: Vec<_> = keymap.bindings().map(|(_, keystroke)| keystroke).collect();
        let unique: HashSet<_> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn the_prefix_yields_to_gui_config_and_typing() {
        let keys = |prefix| DaemonKeys {
            prefixes: vec![keystroke(prefix)],
            bindings: vec![(Command::Tab, Trigger::Prefixed(keystroke("c")))],
            ..no_keys()
        };
        // cmd-b is Toggle Sidebar's catalog default; the prefix takes it.
        let keymap =
            Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys("cmd-b")).unwrap();
        assert!(list(&keymap, Command::ToggleSidebar).is_empty());
        assert!(keymap.is_prefix(&keystroke("cmd-b")));
        // A GUI-configured keystroke keeps its command and disables chords.
        let keymap = Keymap::with_overrides(
            &overrides(&[("themes", one("ctrl-b"))]),
            &Default::default(),
            &keys("ctrl-b"),
        )
        .unwrap();
        assert!(!keymap.is_prefix(&keystroke("ctrl-b")));
        assert_eq!(keymap.chord(&keystroke("c")), None);
        assert_eq!(list(&keymap, Command::Tab), ["cmd-t"]);
        for (prefix, usable) in [
            ("f12", true),
            ("escape", true),
            ("a", false),
            ("shift-a", false),
        ] {
            let keymap =
                Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys(prefix))
                    .unwrap();
            assert_eq!(keymap.is_prefix(&keystroke(prefix)), usable, "{prefix}");
            assert_eq!(keymap.chord(&keystroke("c")).is_some(), usable, "{prefix}");
        }
    }

    fn custom(id: &str, labels: &[&str]) -> ClientShellCommand {
        ClientShellCommand {
            command_id: id.into(),
            binding_label: labels.join(" / "),
            binding_labels: labels.iter().map(|label| (*label).to_owned()).collect(),
            action: herdr_client::protocol::ClientShellCommandAction::Shell,
            description: None,
        }
    }

    #[test]
    fn custom_commands_bind_after_the_keymap() {
        let keymap = Keymap::default();
        let commands = [
            custom("git", &["prefix+y", "ctrl+alt+g"]),
            // Herdr's own chord for New Tab, which the keymap keeps.
            custom("shadowed", &["prefix+c", "cmd+t"]),
            // A bare key would swallow typing, and the prefix is the prefix.
            custom("typing", &["u", "ctrl+b"]),
            custom("legacy", &[]),
        ];
        let find = |typed: &str, prefixed: bool| {
            keymap
                .custom_command(&commands, &keystroke(typed), prefixed)
                .map(|command| command.command_id.as_str())
        };
        assert_eq!(find("y", true), Some("git"));
        assert_eq!(find("ctrl-alt-g", false), Some("git"));
        assert_eq!(find("y", false), None);
        // Herdr's `goto` holds the prefix and g.
        assert_eq!(find("g", true), None);
        assert_eq!(find("ctrl-alt-g", true), None);
        assert_eq!(find("c", true), None);
        assert_eq!(find("cmd-t", false), None);
        assert_eq!(find("u", false), None);
        assert_eq!(find("ctrl-b", false), None);
        assert_eq!(
            keymap.custom_labels(&commands[0]),
            ["ctrl-b y", "ctrl-alt-g"]
        );
        assert!(keymap.custom_labels(&commands[2]).is_empty());
        // `binding_label` drops `prefix+`, so it alone binds nothing.
        let mut old = custom("old", &[]);
        old.binding_label = "g".into();
        assert!(keymap.custom_labels(&old).is_empty());
        assert!(
            keymap
                .custom_command(&[old], &keystroke("g"), true)
                .is_none()
        );
        // Without a usable prefix no chord can reach one.
        let keys = DaemonKeys {
            prefixes: vec![keystroke("a")],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
        assert!(
            keymap
                .custom_command(&commands, &keystroke("y"), true)
                .is_none()
        );
        assert_eq!(keymap.custom_labels(&commands[0]), ["ctrl-alt-g"]);
    }

    #[test]
    fn triggers_tell_chords_from_direct_keystrokes() {
        let keys = DaemonKeys {
            bindings: vec![
                (Command::ResizeMode, Trigger::Prefixed(keystroke("r"))),
                (Command::ResizeMode, Trigger::Direct(keystroke("alt-r"))),
            ],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
        assert!(keymap.triggers(Command::ResizeMode, &keystroke("r"), true));
        assert!(!keymap.triggers(Command::ResizeMode, &keystroke("r"), false));
        assert!(keymap.triggers(Command::ResizeMode, &keystroke("alt-r"), false));
        assert!(!keymap.triggers(Command::ResizeMode, &keystroke("alt-r"), true));
        assert!(!keymap.triggers(Command::Zoom, &keystroke("r"), true));
    }

    #[test]
    fn navigate_keys_never_take_typing_or_picker_keys() {
        let keys = DaemonKeys {
            navigate_up: ["k", "ctrl-p", "pageup", "enter"].map(keystroke).to_vec(),
            navigate_down: vec![keystroke("ctrl-n")],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
        assert_eq!(keymap.navigates_workspace(&keystroke("ctrl-p")), Some(true));
        assert_eq!(keymap.navigates_workspace(&keystroke("pageup")), Some(true));
        assert_eq!(
            keymap.navigates_workspace(&keystroke("ctrl-n")),
            Some(false)
        );
        for typed in ["k", "enter", "up"] {
            assert_eq!(
                keymap.navigates_workspace(&keystroke(typed)),
                None,
                "{typed}"
            );
        }
    }

    #[test]
    fn every_prefix_arms_and_the_first_labels_chords() {
        let keys = DaemonKeys {
            prefixes: vec![keystroke("ctrl-space"), keystroke("ctrl-s")],
            bindings: vec![
                (Command::Tab, Trigger::Prefixed(keystroke("c"))),
                // Any prefix typed after a prefix passes it through instead.
                (Command::NextTab, Trigger::Prefixed(keystroke("ctrl-s"))),
            ],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
        assert!(keymap.is_prefix(&keystroke("ctrl-space")));
        assert!(keymap.is_prefix(&keystroke("ctrl-s")));
        assert!(!keymap.is_prefix(&keystroke("ctrl-b")));
        let first = keystroke("ctrl-space").unparse();
        assert_eq!(keymap.prefix_label().as_deref(), Some(first.as_str()));
        assert_eq!(
            list(&keymap, Command::Tab),
            ["cmd-t".to_owned(), format!("{first} c")]
        );
        assert_eq!(keymap.chord(&keystroke("ctrl-s")), None);
        assert_eq!(list(&keymap, Command::NextTab), ["cmd-shift-]"]);

        // A prefix the GUI config claims, or that would swallow typing, is
        // dropped, and the next one labels chords.
        let keys = DaemonKeys {
            prefixes: vec![keystroke("ctrl-b"), keystroke("a"), keystroke("ctrl-s")],
            bindings: vec![(Command::Tab, Trigger::Prefixed(keystroke("c")))],
            ..no_keys()
        };
        let keymap = Keymap::with_overrides(
            &overrides(&[("themes", one("ctrl-b"))]),
            &Default::default(),
            &keys,
        )
        .unwrap();
        assert!(!keymap.is_prefix(&keystroke("ctrl-b")));
        assert!(!keymap.is_prefix(&keystroke("a")));
        assert!(keymap.is_prefix(&keystroke("ctrl-s")));
        assert_eq!(list(&keymap, Command::Tab), ["cmd-t", "ctrl-s c"]);
    }

    fn pane_keys(entries: &[(&str, &str)]) -> PaneKeys {
        entries
            .iter()
            .map(|(typed, sent)| ((*typed).to_owned(), (*sent).to_owned()))
            .collect()
    }

    fn sent<'a>(keymap: &'a Keymap, typed: &str) -> Option<&'a Keystroke> {
        keymap.pane_key(&keystroke(typed))
    }

    #[test]
    fn default_pane_keys_follow_ghostty_on_macos() {
        let defaults = DEFAULT_PANE_KEYS
            .iter()
            .map(|(typed, sent)| (keystroke(typed), keystroke(sent)));
        assert!(
            defaults
                .clone()
                .all(|(_, sent)| crate::terminal::reaches_pane(&sent))
        );
        let keymap = Keymap::default();
        assert_eq!(
            keymap.pane_keys.len(),
            if cfg!(target_os = "macos") { 3 } else { 0 }
        );
        // Defaults are replaced by keystroke, not spelling, or removed.
        let resolved = resolve_pane_keys(
            &pane_keys(&[
                ("cmd-backspace", ""),
                ("cmd-left", "home"),
                ("ctrl-shift-enter", "alt-enter"),
            ]),
            defaults,
        )
        .unwrap();
        assert_eq!(
            resolved,
            [
                (keystroke("cmd-right"), keystroke("ctrl-e")),
                (keystroke("cmd-left"), keystroke("home")),
                (keystroke("ctrl-shift-enter"), keystroke("alt-enter")),
            ]
        );
    }

    #[test]
    fn pane_keys_take_keystrokes_from_default_and_daemon_commands() {
        let keys = DaemonKeys {
            prefixes: vec![keystroke("ctrl-b"), keystroke("ctrl-s")],
            bindings: vec![(Command::Themes, Trigger::Direct(keystroke("ctrl-alt-t")))],
            ..no_keys()
        };
        let table = pane_keys(&[
            ("cmd-k", "ctrl-l"),
            ("ctrl-alt-t", "f5"),
            ("ctrl-b", "ctrl-a"),
        ]);
        let keymap = Keymap::with_overrides(&BTreeMap::new(), &table, &keys).unwrap();
        assert_eq!(sent(&keymap, "cmd-k"), Some(&keystroke("ctrl-l")));
        assert_eq!(keymap.primary(Command::ClearPane), "");
        assert_eq!(keymap.primary(Command::Themes), "");
        assert!(!keymap.is_prefix(&keystroke("ctrl-b")));
        assert_eq!(keymap.prefix_label(), Some(keystroke("ctrl-s").unparse()));
        assert!(keymap.bindings().all(|(_, label)| label != "cmd-k"));
        assert_eq!(sent(&keymap, "cmd-j"), None);
    }

    #[test]
    fn pane_keys_reject_typing_unsendable_targets_and_configured_commands() {
        let resolve = |entries: &[(&str, &str)], bindings: &[(&str, Binding)]| {
            Keymap::with_overrides(&overrides(bindings), &pane_keys(entries), &no_keys())
        };
        assert!(matches!(
            resolve(&[("a", "ctrl-a")], &[]),
            Err(Error::PaneKeyWithoutModifier(_))
        ));
        assert!(matches!(
            resolve(&[("shift-space", "ctrl-a")], &[]),
            Err(Error::PaneKeyWithoutModifier(_))
        ));
        assert!(matches!(
            resolve(&[("cmd-left", "cmd-a")], &[]),
            Err(Error::UnsendablePaneKey { .. })
        ));
        assert!(matches!(
            resolve(&[("cmd-left", "ctrl-nope-a")], &[]),
            Err(Error::InvalidPaneKey { .. })
        ));
        assert!(matches!(
            resolve(&[("cmd-left", "ctrl-a"), ("super-left", "ctrl-e")], &[]),
            Err(Error::DuplicatePaneKey(_))
        ));
        assert!(matches!(
            resolve(&[("cmd-y", "ctrl-a")], &[("new_tab", one("cmd-y"))]),
            Err(Error::PaneKeyBound {
                command: "new_tab",
                ..
            })
        ));
        // A key that cannot be text needs no modifier.
        assert!(resolve(&[("shift-enter", "alt-enter"), ("f13", "ctrl-u")], &[]).is_ok());
        // Counted before parsing, so a huge table is refused cheaply.
        let many: PaneKeys = (0..=MAX_PANE_KEYS)
            .map(|n| (format!("{}cmd-k", " ".repeat(n)), "ctrl-a".to_owned()))
            .collect();
        assert!(matches!(
            Keymap::with_overrides(&BTreeMap::new(), &many, &no_keys()),
            Err(Error::TooManyPaneKeys(MAX_PANE_KEYS))
        ));
    }
}

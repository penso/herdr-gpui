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
}

impl Default for Keymap {
    /// The catalog under Herdr's own default `[keys]`, as when neither config
    /// file names a keystroke.
    fn default() -> Self {
        Self::layer(
            vec![None; COMMANDS.len()],
            &Claimed::new(),
            &DaemonKeys::default(),
        )
    }
}

impl Keymap {
    /// Layers the daemon's `keys` and then `overrides` over the defaults. A
    /// keystroke the GUI config assigns moves to that command, so rebinding
    /// one key never requires unbinding its default or daemon owner too; two
    /// configured commands claiming it is an error. A command the GUI config
    /// names keeps exactly the keystrokes listed there.
    pub(crate) fn with_overrides(
        overrides: &BTreeMap<String, Binding>,
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
        Ok(Self::layer(configured, &claimed, keys))
    }

    /// Herdr owns and validates its own file, so a daemon binding this client
    /// cannot honor, or that collides with one already placed, is skipped
    /// rather than reported: the first daemon binding for a keystroke wins,
    /// as does any GUI-configured keystroke over the daemon's.
    fn layer(configured: Vec<Option<Vec<String>>>, claimed: &Claimed, keys: &DaemonKeys) -> Self {
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
        }
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
mod tests;

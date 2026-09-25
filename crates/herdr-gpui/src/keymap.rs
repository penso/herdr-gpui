//! The keystrokes bound to each catalog command: the defaults in
//! `controls::COMMANDS`, with the config file's `[keybindings]` table layered
//! on top. The palette, keybindings page, menu bar, and GPUI keymap all read
//! this one resolved answer.

use crate::{
    Error, Result,
    controls::{COMMANDS, Command},
};
use gpui_kit::{Keystroke, Modifiers};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

/// A command may carry an alias or two, never an unbounded list.
const MAX_KEYSTROKES: usize = 8;

/// One entry of the config's `[keybindings]` table: a keystroke, or a list of
/// them. An empty string or list leaves the command unbound.
#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    /// Parallel to `COMMANDS`, primary keystroke first, as the user wrote them.
    shortcuts: Vec<Vec<String>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            shortcuts: COMMANDS
                .iter()
                .map(|info| info.shortcuts.iter().map(|&s| s.to_owned()).collect())
                .collect(),
        }
    }
}

impl Keymap {
    /// Layers `overrides` over the defaults. A keystroke the config assigns
    /// moves to that command, so rebinding one key never requires unbinding
    /// its default owner too; two configured commands claiming it is an error.
    pub(crate) fn with_overrides(overrides: &BTreeMap<String, Binding>) -> Result<Self> {
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
        let mut claimed: HashMap<(Modifiers, String), &'static str> = HashMap::new();
        for (info, keystrokes) in COMMANDS.iter().zip(&configured) {
            for keystroke in keystrokes.iter().flatten() {
                let parsed = parse(info.name, keystroke)?;
                if let Some(first) = claimed.insert((parsed.modifiers, parsed.key), info.name)
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
        let shortcuts = COMMANDS
            .iter()
            .zip(configured)
            .map(|(info, keystrokes)| {
                keystrokes.unwrap_or_else(|| {
                    info.shortcuts
                        .iter()
                        .filter(|shortcut| {
                            Keystroke::parse(shortcut).is_ok_and(|parsed| {
                                !claimed.contains_key(&(parsed.modifiers, parsed.key))
                            })
                        })
                        .map(|&shortcut| shortcut.to_owned())
                        .collect()
                })
            })
            .collect();
        Ok(Self { shortcuts })
    }

    /// Every keystroke bound to `command`, primary first.
    pub fn shortcuts(&self, command: Command) -> &[String] {
        COMMANDS
            .iter()
            .position(|info| info.command == command)
            .map_or(&[], |index| &self.shortcuts[index])
    }

    /// The keystroke shown beside `command`, or `""` when it is unbound.
    pub fn primary(&self, command: Command) -> &str {
        self.shortcuts(command).first().map_or("", String::as_str)
    }

    /// Each bound keystroke with the command it runs, in catalog order.
    pub fn bindings(&self) -> impl Iterator<Item = (Command, &str)> {
        COMMANDS
            .iter()
            .zip(&self.shortcuts)
            .flat_map(|(info, keystrokes)| {
                keystrokes
                    .iter()
                    .map(move |keystroke| (info.command, keystroke.as_str()))
            })
    }
}

fn parse(command: &'static str, keystroke: &str) -> Result<Keystroke> {
    let parsed = Keystroke::parse(keystroke).map_err(|source| Error::InvalidKeystroke {
        command,
        keystroke: keystroke.to_owned(),
        source,
    })?;
    let modifiers = parsed.modifiers;
    if !(modifiers.platform || modifiers.control || modifiers.alt || modifiers.function) {
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

    #[test]
    fn defaults_follow_the_catalog() {
        let keymap = Keymap::default();
        assert_eq!(keymap.shortcuts(Command::Tab), ["cmd-t"]);
        assert_eq!(keymap.primary(Command::Tab), "cmd-t");
        assert_eq!(keymap.primary(Command::Workspace), "cmd-shift-n");
        assert_eq!(keymap.primary(Command::Themes), "");
        assert_eq!(
            Keymap::with_overrides(&BTreeMap::new()).unwrap(),
            Keymap::default()
        );
        let bound: usize = COMMANDS.iter().map(|info| info.shortcuts.len()).sum();
        assert_eq!(keymap.bindings().count(), bound);
    }

    #[test]
    fn overrides_replace_lists_and_empty_values_unbind() {
        let keymap = Keymap::with_overrides(&overrides(&[
            ("new_workspace", one("cmd-alt-t")),
            (
                "themes",
                Binding::Many(vec!["ctrl-shift-t".into(), " cmd-k ".into()]),
            ),
            ("close_pane", one("")),
            ("toggle_sidebar", Binding::Many(Vec::new())),
        ]))
        .unwrap();
        assert_eq!(keymap.shortcuts(Command::Workspace), ["cmd-alt-t"]);
        assert_eq!(keymap.shortcuts(Command::Themes), ["ctrl-shift-t", "cmd-k"]);
        assert!(keymap.shortcuts(Command::ClosePane).is_empty());
        assert!(keymap.shortcuts(Command::ToggleSidebar).is_empty());
        assert_eq!(keymap.shortcuts(Command::Tab), ["cmd-t"]);
    }

    /// Taking a default keystroke must not force the user to also unbind it
    /// from the command that shipped with it.
    #[test]
    fn configured_keystroke_moves_from_its_default_owner() {
        let keymap =
            Keymap::with_overrides(&overrides(&[("new_workspace", one("cmd-t"))])).unwrap();
        assert_eq!(keymap.shortcuts(Command::Workspace), ["cmd-t"]);
        assert!(keymap.shortcuts(Command::Tab).is_empty());
        // Spelling differences still name the same keystroke.
        let keymap = Keymap::with_overrides(&overrides(&[("about", one("CMD-shift-P"))])).unwrap();
        assert!(keymap.shortcuts(Command::Palette).is_empty());
        let keys: Vec<_> = keymap.bindings().map(|(_, keystroke)| keystroke).collect();
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn invalid_configuration_reports_the_command() {
        let error =
            |entries: &[(&str, Binding)]| Keymap::with_overrides(&overrides(entries)).unwrap_err();
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
        Keymap::with_overrides(&overrides(&[(
            "new_tab",
            Binding::Many(vec!["cmd-k".into(), "cmd-k".into()]),
        )]))
        .unwrap();
    }
}

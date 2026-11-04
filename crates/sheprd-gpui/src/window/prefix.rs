//! Herdr's prefix chords, as the TUI types them: the prefix arms the window,
//! and the next keystroke runs its chord's command or, when nothing is bound
//! to it, is swallowed. Typing the prefix twice sends it on to the focused
//! element as ordinary input, and Escape simply cancels. A chord typed in a
//! menu's text field closes the menu and runs, as it would from the terminal.
//!
//! The daemon's custom command shortcuts are matched here too, after the
//! keymap's own, and Herdr's resize mode claims every key until it ends.
//! A shifted key is also tried with the Shift GPUI dropped restored, after
//! every lookup has missed the key as GPUI named it.

use super::SheprdWindow;
use crate::controls::Command;
use gpui::{Context, Keystroke, Subscription, Window};
use sheprd_client::protocol::ClientShellCommandAction;

mod shifted_keys;

pub(crate) use shifted_keys::KeyChars;
use shifted_keys::Typed;
pub(super) use shifted_keys::current_layout;

/// What a keystroke is bound to here.
enum Bound {
    Prefix,
    /// Only after the prefix.
    Chord(Command),
    Custom(String, ClientShellCommandAction),
}

impl SheprdWindow {
    /// GPUI calls interceptors before any binding or key handler sees the
    /// keystroke, so an armed prefix claims the next key wherever focus is.
    /// Interceptors are app-wide; each window answers only for itself.
    pub(crate) fn intercept_prefix(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let own = window.window_handle().window_id();
        let view = cx.weak_entity();
        cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() != own {
                return;
            }
            let _ = view.update(cx, |this, cx| {
                if !matches!(
                    event.keystroke.key.as_str(),
                    "shift" | "shiftleft" | "shiftright"
                ) {
                    this.shift_taps.cancel();
                }
                // Everything matched here sees the function key's name, and
                // a shifted key's restored Shift: the prefix, its chords,
                // custom commands, and resize mode. GPUI's own bindings
                // still see the key as GPUI named it.
                let named = super::function_keys::named(&event.keystroke, this.key_press);
                let keystroke = named.as_ref().unwrap_or(&event.keystroke);
                let restored = shifted_keys::restore(keystroke, this.key_press, this.key_layout);
                let typed = Typed::new(keystroke, restored);
                this.prefix_keystroke(&typed, window, cx);
            });
        })
    }

    fn prefix_keystroke(&mut self, typed: &Typed<'_>, window: &mut Window, cx: &mut Context<Self>) {
        if self.resize_mode {
            self.resize_keystroke(typed, window, cx);
            return;
        }
        // Every binding is tried on the key as GPUI named it before any is
        // tried on the restored key, so a binding on the symbol itself, such
        // as a custom command on `prefix+!`, still wins over one on
        // `prefix+shift+1`, and over a prefix on the restored key.
        let armed = self.prefix_armed;
        let bound = if armed {
            typed.find(|keystroke| self.bound(keystroke, true))
        } else {
            // Without the prefix, a key GPUI's bindings or a pane key hold
            // as GPUI named it is theirs; restoring its Shift must not take
            // it from them for the prefix or a custom command.
            let held = |keystroke: &Keystroke| self.keymap().holds_directly(keystroke);
            typed.find_unheld(|keystroke| self.bound(keystroke, false), held)
        };
        if !armed {
            match bound {
                Some(Bound::Prefix) => {
                    self.prefix_armed = true;
                    cx.stop_propagation();
                    cx.notify();
                }
                Some(Bound::Custom(id, action)) => {
                    cx.stop_propagation();
                    self.invoke_custom_command(&id, action, cx);
                }
                Some(Bound::Chord(_)) | None => {}
            }
            return;
        }
        self.prefix_armed = false;
        cx.notify();
        let Some(bound) = bound else {
            cx.stop_propagation();
            return;
        };
        // Typed twice, the prefix goes on as ordinary input.
        if matches!(bound, Bound::Prefix) {
            return;
        }
        cx.stop_propagation();
        // Commands wait while a menu page holds input, so the chord ends it
        // first. Run directly rather than through focus, which the menu held.
        if self.menu.page.is_some() {
            self.dismiss_menu(window, cx);
        }
        match bound {
            Bound::Chord(command) => self.command(command, window, cx),
            Bound::Custom(id, action) => self.invoke_custom_command(&id, action, cx),
            Bound::Prefix => {}
        }
    }

    /// What `keystroke` is bound to: the prefix, then, after the prefix, the
    /// keymap's chord, then a custom command. Without the prefix, a custom
    /// command runs only while no menu page holds input.
    fn bound(&self, keystroke: &Keystroke, armed: bool) -> Option<Bound> {
        let keymap = self.keymap();
        if keymap.is_prefix(keystroke) {
            return Some(Bound::Prefix);
        }
        if armed && let Some(command) = keymap.chord(keystroke) {
            return Some(Bound::Chord(command));
        }
        if !armed && self.menu.page.is_some() {
            return None;
        }
        self.custom_command(keystroke, armed)
            .map(|(id, action)| Bound::Custom(id, action))
    }

    /// The selected daemon's custom command `keystroke` runs.
    fn custom_command(
        &self,
        keystroke: &Keystroke,
        prefixed: bool,
    ) -> Option<(String, ClientShellCommandAction)> {
        // As the palette and shortcut reference do, offer nothing while the
        // snapshot may be stale, so the key reaches the terminal instead.
        let snapshot = self
            .live
            .snapshot
            .as_ref()
            .filter(|_| self.live.status.is_connected())?;
        self.keymap()
            .custom_command(&snapshot.commands, keystroke, prefixed)
            .map(|command| (command.command_id.clone(), command.action))
    }

    /// Herdr's resize mode: h, j, k, l or the arrows resize the focused pane,
    /// Escape, Enter, or the mode's own shortcut ends it, and anything else
    /// is swallowed. Platform shortcuts such as quitting still work, and a
    /// menu, which takes keys of its own, ends the mode.
    fn resize_keystroke(&mut self, typed: &Typed<'_>, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = typed.keystroke();
        if self.menu.page.is_some() {
            self.resize_mode = false;
            cx.notify();
            return;
        }
        if keystroke.modifiers.platform {
            return;
        }
        cx.stop_propagation();
        let keymap = self.keymap();
        if matches!(keystroke.key.as_str(), "escape" | "enter")
            || typed.matches(|keystroke| {
                keymap.triggers(Command::ResizeMode, keystroke, true)
                    || keymap.triggers(Command::ResizeMode, keystroke, false)
            })
        {
            self.resize_mode = false;
            cx.notify();
            return;
        }
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.function {
            return;
        }
        let command = match keystroke.key.as_str() {
            "h" | "left" => Command::ResizeLeft,
            "j" | "down" => Command::ResizeDown,
            "k" | "up" => Command::ResizeUp,
            "l" | "right" => Command::ResizeRight,
            _ => return,
        };
        self.command(command, window, cx);
    }

    /// Leaving the window abandons a half-typed chord, as the TUI's prefix
    /// mode does not outlive its client, and ends resize mode.
    pub(super) fn disarm_prefix(&mut self) {
        self.prefix_armed = false;
        self.resize_mode = false;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::{
        keymap::{DaemonKeys, Keymap},
        state::ConnectionStatus,
    };
    use gpui::{Keystroke, TestAppContext, VisualTestContext};
    use sheprd_client::protocol::{ClientShellCommand, ClientShellCommandAction};
    use std::sync::Arc;

    /// Whether something handled the keystroke. Nothing binds `cmd-y` and
    /// the terminal ignores cmd keys, so only the prefix can claim it.
    fn press(keystroke: &str, cx: &mut VisualTestContext) -> bool {
        let handled = cx.update(|window, cx| {
            window.dispatch_keystroke(Keystroke::parse(keystroke).unwrap(), cx)
        });
        cx.run_until_parked();
        handled
    }

    #[gpui::test]
    fn the_prefix_runs_chords_and_swallows_other_keys(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let table: toml::Table = "prefix = ['cmd+j', 'cmd+u']\ntoggle_sidebar = 'prefix+cmd+b'"
            .parse()
            .unwrap();
        view.update(cx, |view, _| {
            view.config.keybindings = Keymap::with_overrides(
                &Default::default(),
                &Default::default(),
                &DaemonKeys::from_table(Some(&table)),
            )
            .unwrap();
        });
        let state = |cx: &mut VisualTestContext| {
            view.read_with(cx, |view, _| (view.prefix_armed, view.sidebar_visible))
        };
        cx.update(|window, cx| {
            window.focus(&view.read(cx).focus.clone(), cx);
            window.draw(cx).clear(cx);
        });
        assert!(!press("cmd-y", cx));
        assert!(
            !press("cmd-b", cx),
            "the chord's key alone is not a binding"
        );
        assert_eq!(state(cx), (false, true));

        assert!(press("cmd-j", cx));
        assert_eq!(state(cx), (true, true));
        assert!(press("cmd-b", cx));
        assert_eq!(state(cx), (false, false), "the chord ran");

        // Any configured prefix arms the same chords.
        assert!(press("cmd-u", cx));
        assert_eq!(state(cx), (true, false));
        assert!(press("cmd-b", cx));
        assert_eq!(state(cx), (false, true), "the second prefix ran the chord");
        assert!(press("cmd-j", cx));
        assert!(!press("cmd-u", cx), "another prefix passes through too");
        assert_eq!(state(cx), (false, true));
        assert!(press("cmd-u", cx));
        assert!(press("cmd-b", cx));
        assert_eq!(state(cx), (false, false));

        // An unbound key after the prefix goes nowhere, as in the TUI.
        assert!(press("cmd-j", cx));
        assert!(press("cmd-y", cx));
        assert_eq!(state(cx), (false, false));
        assert!(!press("cmd-y", cx), "only the next key is claimed");

        // The prefix twice passes the second one through.
        assert!(press("cmd-j", cx));
        assert!(!press("cmd-j", cx));
        assert_eq!(state(cx), (false, false));

        assert!(press("cmd-j", cx));
        assert!(press("escape", cx));
        assert_eq!(state(cx), (false, false));
        assert!(press("cmd-j", cx));
        assert!(press("cmd-b", cx));
        assert_eq!(state(cx), (false, true));

        // From a menu's focused text field, the chord closes the menu and runs.
        view.update_in(cx, |view, window, cx| view.open_keybinds(window, cx));
        cx.run_until_parked();
        let search_focused = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                let view = view.read(cx);
                view.menu.page.is_some()
                    && view
                        .menu
                        .keybinds_search
                        .as_ref()
                        .is_some_and(|search| search.read(cx).focus.is_focused(window))
            })
        };
        assert!(search_focused(cx));
        assert!(press("cmd-j", cx));
        assert!(search_focused(cx), "the prefix alone leaves the menu open");
        assert!(press("cmd-b", cx));
        assert_eq!(state(cx), (false, false));
        view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));

        // An unbound key there is swallowed and leaves the menu open.
        view.update_in(cx, |view, window, cx| view.open_keybinds(window, cx));
        cx.run_until_parked();
        assert!(press("cmd-j", cx));
        assert!(press("cmd-y", cx));
        assert!(search_focused(cx));
    }

    /// A prefix that types text, such as `prefix = "@"` in the daemon's
    /// `[keys]`, arms however the keyboard reports it, and typing it twice
    /// sends it on.
    #[gpui::test]
    fn a_typing_prefix_arms_everywhere_and_twice_types_it(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let table: toml::Table = "prefix = '@'\ntoggle_sidebar = 'prefix+b'".parse().unwrap();
        view.update(cx, |view, _| {
            view.config.keybindings = Keymap::with_overrides(
                &Default::default(),
                &Default::default(),
                &DaemonKeys::from_table(Some(&table)),
            )
            .unwrap();
        });
        let state = |cx: &mut VisualTestContext| {
            view.read_with(cx, |view, _| (view.prefix_armed, view.sidebar_visible))
        };
        cx.update(|window, cx| {
            window.focus(&view.read(cx).focus.clone(), cx);
            window.draw(cx).clear(cx);
        });
        for typed in ["@", "shift-2->@"] {
            let visible = state(cx).1;
            assert!(press(typed, cx), "{typed}");
            assert_eq!(state(cx), (true, visible), "{typed}");
            assert!(press("b", cx), "{typed}");
            assert_eq!(state(cx), (false, !visible), "{typed} ran the chord");
        }

        // In a text field too, as in the TUI: twice types it once, and a
        // chord closes the menu and runs.
        let menu_open =
            |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.menu.page.is_some());
        let search = |cx: &mut VisualTestContext| {
            view.read_with(cx, |view, cx| {
                view.menu
                    .keybinds_search
                    .as_ref()
                    .map(|search| search.read(cx).text().to_owned())
            })
        };
        for typed in ["@", "shift-2->@"] {
            view.update_in(cx, |view, window, cx| view.open_keybinds(window, cx));
            cx.run_until_parked();
            assert_eq!(search(cx).as_deref(), Some(""), "{typed}");
            let visible = state(cx).1;
            assert!(press(typed, cx), "{typed}");
            assert_eq!(state(cx), (true, visible), "{typed}");
            assert_eq!(
                search(cx).as_deref(),
                Some(""),
                "{typed} arms, types nothing"
            );
            press(typed, cx);
            assert_eq!(state(cx), (false, visible), "{typed}");
            assert_eq!(search(cx).as_deref(), Some("@"), "{typed} twice types one");
            assert!(menu_open(cx), "{typed}");

            assert!(press(typed, cx), "{typed}");
            assert!(press("b", cx), "{typed}");
            assert_eq!(state(cx), (false, !visible), "{typed}");
            assert!(!menu_open(cx), "{typed} chord closed the menu");
        }
    }

    /// The shortcut reference and palette hide custom commands while the
    /// snapshot may be stale, so dispatch must not run them then either.
    #[gpui::test]
    fn custom_commands_run_only_while_connected(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let typed = Keystroke::parse("ctrl-alt-y").unwrap();
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).commands =
                vec![ClientShellCommand {
                    command_id: "plugin.y".into(),
                    description: None,
                    action: ClientShellCommandAction::PluginAction,
                    binding_label: "ctrl+alt+y".into(),
                    binding_labels: vec!["ctrl+alt+y".into()],
                }];
            for (status, runs) in [
                (ConnectionStatus::Connected, true),
                (ConnectionStatus::AwaitingSnapshot, true),
                (ConnectionStatus::Connecting, false),
                (ConnectionStatus::Disconnected, false),
                (ConnectionStatus::Detached, false),
            ] {
                view.live.status = status;
                assert_eq!(
                    view.custom_command(&typed, false).is_some(),
                    runs,
                    "{status:?}"
                );
            }
        });
    }
}

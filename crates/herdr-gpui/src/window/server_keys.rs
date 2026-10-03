//! A saved device can opt into its server's keybindings, as
//! `herdr --remote --remote-keybindings server` does for the TUI. The keymap
//! then follows the selected endpoint: that host's published `[keys]` profile,
//! with this GUI's own `[keybindings]` still layered on top. Everything else,
//! themes and sidebar included, stays local by upstream design.

use super::HerdrWindow;
use crate::{
    Result,
    config::KeybindingSource,
    keymap::{Binding, DaemonKeys, Keymap, PaneKeys},
};
use gpui::{App, Context, Global};
use std::collections::BTreeMap;

/// The keymap a window built from its selected endpoint's server profile, and
/// what it was built from, so a new snapshot only rebuilds it when the profile
/// or the GUI overrides actually changed.
pub(crate) struct ServerKeymap {
    endpoint: String,
    profile: Option<String>,
    overrides: BTreeMap<String, Binding>,
    pane_keys: PaneKeys,
    /// An error leaves the window on its local keymap, as Herdr keeps the
    /// bindings it had when a profile cannot be applied.
    keymap: Result<Keymap>,
}

impl ServerKeymap {
    fn build(
        endpoint: &str,
        profile: Option<&str>,
        overrides: &BTreeMap<String, Binding>,
        pane_keys: &PaneKeys,
    ) -> Self {
        let keymap = DaemonKeys::from_profile(profile)
            .and_then(|keys| Keymap::with_overrides(overrides, pane_keys, &keys));
        if let Err(error) = &keymap {
            tracing::warn!(endpoint, %error, "Using local keybindings for this device");
        }
        Self {
            endpoint: endpoint.to_owned(),
            profile: profile.map(str::to_owned),
            overrides: overrides.clone(),
            pane_keys: pane_keys.clone(),
            keymap,
        }
    }

    fn built_from(
        &self,
        endpoint: &str,
        profile: Option<&str>,
        overrides: &BTreeMap<String, Binding>,
        pane_keys: &PaneKeys,
    ) -> bool {
        self.endpoint == endpoint
            && self.profile.as_deref() == profile
            && self.overrides == *overrides
            && self.pane_keys == *pane_keys
    }
}

/// The server keymap of the active window's endpoint, when it uses one. GPUI
/// bindings are app-wide, so the active window decides which direct keys are
/// bound; every window answers its own prefix chords.
#[derive(Default)]
pub(crate) struct ActiveServerKeymap(pub(crate) Option<Keymap>);

impl Global for ActiveServerKeymap {}

impl HerdrWindow {
    /// The keymap this window's keys and labels follow.
    pub(crate) fn keymap(&self) -> &Keymap {
        self.server_keymap().unwrap_or(&self.config.keybindings)
    }

    fn server_keymap(&self) -> Option<&Keymap> {
        self.server_keys
            .as_ref()
            .and_then(|keys| keys.keymap.as_ref().ok())
    }

    /// Why the endpoint is on local keybindings although it asked for its
    /// server's, while it is the selected one.
    pub(crate) fn server_keybindings_error(&self, endpoint: &str) -> Option<String> {
        self.server_keys
            .as_ref()
            .filter(|keys| keys.endpoint == endpoint)
            .and_then(|keys| keys.keymap.as_ref().err())
            .map(ToString::to_string)
    }

    /// Rebuilds the server keymap when the selection, its published profile,
    /// or the opt-in changed. Until the first snapshot arrives there is no
    /// profile to judge, so the local keymap applies without a diagnostic.
    pub(crate) fn sync_server_keymap(&mut self, cx: &mut Context<Self>) {
        let endpoint = &self.endpoints[self.selected_endpoint].id;
        let wanted = self.config.keybinding_source(endpoint) == KeybindingSource::Server;
        let next =
            match (wanted, &self.live.snapshot) {
                (true, Some(snapshot)) => {
                    let profile = snapshot.server_keybindings_toml.as_deref();
                    let overrides = &self.config.keybinding_overrides;
                    let pane_keys = &self.config.pane_keys;
                    if self.server_keys.as_ref().is_some_and(|keys| {
                        keys.built_from(endpoint, profile, overrides, pane_keys)
                    }) {
                        return;
                    }
                    Some(ServerKeymap::build(endpoint, profile, overrides, pane_keys))
                }
                _ if self.server_keys.is_none() => return,
                _ => None,
            };
        let before = self.server_keymap().cloned();
        self.server_keys = next;
        if self.server_keymap() == before.as_ref() {
            // A diagnostic may still have changed.
            cx.notify();
            return;
        }
        // A chord armed under the old prefix must not complete under the new one.
        self.disarm_prefix();
        if self.active {
            self.publish_server_keymap(cx);
        }
        cx.notify();
    }

    /// Makes this window's direct keys the bound ones. Called when it becomes
    /// the active window and when its keymap changes while active.
    pub(super) fn publish_server_keymap(&self, cx: &mut App) {
        let keymap = self.server_keymap().cloned();
        if cx
            .try_global::<ActiveServerKeymap>()
            .map_or(keymap.is_none(), |active| active.0 == keymap)
        {
            return;
        }
        cx.set_global(ActiveServerKeymap(keymap));
        crate::actions::rebind_keys(cx);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{Command, Error, config::DeviceSettings, keymap::Binding};
    use gpui::{Entity, Keystroke, TestAppContext, VisualTestContext};
    use std::sync::Arc;

    const ID: &str = "0123456789abcdef0123456789abcdef";
    const HOST: &str = "ssh:0123456789abcdef0123456789abcdef";
    const PROFILE: &str = "[keys]\nprefix = \"cmd+j\"\ntoggle_sidebar = \"prefix+cmd+b\"\n";

    fn key(text: &str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    /// A window with Local and one saved device, which is not enabled so the
    /// fixture never dials it.
    fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, _| {
            view.endpoints.push(crate::endpoint::Endpoint::new(
                HOST.into(),
                "box".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "you@box".into(),
                    session: "default".into(),
                },
                false,
            ));
        });
        (view, cx)
    }

    /// Selects an endpoint as the poll loop would leave it: its snapshot,
    /// publishing `profile`, is the window's live one.
    fn select(
        view: &mut HerdrWindow,
        index: usize,
        profile: Option<&str>,
        cx: &mut Context<HerdrWindow>,
    ) {
        view.selected_endpoint = index;
        let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
        snapshot.server_keybindings_toml = profile.map(str::to_owned);
        view.live.snapshot = Some(Arc::new(snapshot));
        view.sync_server_keymap(cx);
    }

    fn opt_in(view: &mut HerdrWindow, source: KeybindingSource) {
        view.config.devices.insert(
            ID.into(),
            DeviceSettings {
                keybindings: source,
            },
        );
    }

    fn active_global(cx: &mut VisualTestContext) -> Option<Keymap> {
        cx.update(|_, cx| {
            cx.try_global::<ActiveServerKeymap>()
                .and_then(|active| active.0.clone())
        })
    }

    #[gpui::test]
    fn the_keymap_follows_the_selected_device_only_when_it_opted_in(cx: &mut TestAppContext) {
        let (view, cx) = window(cx);
        view.update(cx, |view, cx| {
            view.active = true;
            // Opted out by default: the device's profile is ignored.
            select(view, 1, Some(PROFILE), cx);
            assert!(view.server_keys.is_none());
            assert!(view.keymap().is_prefix(&key("ctrl-b")));

            opt_in(view, KeybindingSource::Server);
            view.sync_server_keymap(cx);
            assert!(view.keymap().is_prefix(&key("cmd-j")));
            assert!(!view.keymap().is_prefix(&key("ctrl-b")));
            assert_eq!(
                view.keymap().chord(&key("cmd-b")),
                Some(Command::ToggleSidebar)
            );
            // The window's config, which other windows and reloads compare,
            // keeps the local keymap.
            assert!(view.config.keybindings.is_prefix(&key("ctrl-b")));
        });
        let server = view.read_with(cx, |view, _| view.keymap().clone());
        assert_eq!(
            active_global(cx),
            Some(server),
            "the active window binds its keys"
        );

        view.update(cx, |view, cx| {
            // Moving focus to Local switches back, and so does opting out.
            select(view, 0, Some(PROFILE), cx);
            assert!(view.keymap().is_prefix(&key("ctrl-b")));
            select(view, 1, Some(PROFILE), cx);
            assert!(view.keymap().is_prefix(&key("cmd-j")));
            opt_in(view, KeybindingSource::Local);
            view.sync_server_keymap(cx);
            assert!(view.server_keys.is_none());
            assert!(view.keymap().is_prefix(&key("ctrl-b")));
        });
        assert_eq!(active_global(cx), None);
    }

    #[gpui::test]
    fn an_unusable_profile_falls_back_to_local_and_says_why(cx: &mut TestAppContext) {
        let (view, cx) = window(cx);
        view.update(cx, |view, cx| {
            opt_in(view, KeybindingSource::Server);
            // No snapshot yet: nothing to judge, so no diagnostic.
            view.selected_endpoint = 1;
            view.live.snapshot = None;
            view.sync_server_keymap(cx);
            assert!(view.server_keys.is_none());
            assert_eq!(view.server_keybindings_error(HOST), None);

            for (profile, expected) in [
                (None, "the host did not publish its keybindings"),
                (Some("[keys"), "the host's keybindings are not valid TOML"),
                (
                    Some("prefix = 'cmd+j'"),
                    "the host's keybindings have no [keys] table",
                ),
            ] {
                select(view, 1, profile, cx);
                assert!(view.keymap().is_prefix(&key("ctrl-b")), "{profile:?}");
                let error = view.server_keybindings_error(HOST).unwrap();
                assert!(error.starts_with(expected), "{error}");
                assert!(matches!(
                    view.server_keys.as_ref().unwrap().keymap,
                    Err(Error::ServerKeybindingsMissing
                        | Error::ServerKeybindingsParse(_)
                        | Error::ServerKeybindingsNoKeys)
                ));
            }
            // A later valid profile replaces the fallback.
            select(view, 1, Some(PROFILE), cx);
            assert_eq!(view.server_keybindings_error(HOST), None);
            assert!(view.keymap().is_prefix(&key("cmd-j")));
            // The diagnostic belongs to the device it describes.
            select(view, 1, None, cx);
            assert_eq!(view.server_keybindings_error("local"), None);
        });
    }

    #[gpui::test]
    fn gui_overrides_still_layer_over_server_keys(cx: &mut TestAppContext) {
        let (view, cx) = window(cx);
        view.update(cx, |view, cx| {
            opt_in(view, KeybindingSource::Server);
            select(view, 1, Some(PROFILE), cx);
            assert_eq!(view.keymap().primary(Command::Tab), "cmd-t");
            // A reload with new overrides rebuilds the server keymap.
            view.config
                .keybinding_overrides
                .insert("new_tab".into(), Binding::One("cmd-y".into()));
            view.sync_server_keymap(cx);
            assert_eq!(view.keymap().primary(Command::Tab), "cmd-y");
            assert!(view.keymap().is_prefix(&key("cmd-j")));
        });
    }

    #[gpui::test]
    fn the_server_prefix_runs_chords_and_a_switch_disarms_it(cx: &mut TestAppContext) {
        let (view, cx) = window(cx);
        let press = |keystroke: &str, cx: &mut VisualTestContext| {
            let handled = cx.update(|window, cx| window.dispatch_keystroke(key(keystroke), cx));
            cx.run_until_parked();
            handled
        };
        view.update(cx, |view, cx| {
            opt_in(view, KeybindingSource::Server);
            select(view, 1, Some(PROFILE), cx);
        });
        cx.update(|window, cx| {
            window.focus(&view.read(cx).focus.clone(), cx);
            window.draw(cx).clear(cx);
        });
        let state = |cx: &mut VisualTestContext| {
            view.read_with(cx, |view, _| (view.prefix_armed, view.sidebar_visible))
        };
        assert!(press("cmd-j", cx));
        assert!(press("cmd-b", cx));
        assert_eq!(state(cx), (false, false), "the server chord ran");

        // A prefix armed under the server keymap does not survive a switch.
        assert!(press("cmd-j", cx));
        view.update(cx, |view, cx| select(view, 0, Some(PROFILE), cx));
        assert_eq!(state(cx), (false, false));
        assert!(!press("cmd-j", cx), "Local's prefix is ctrl-b");
    }
}

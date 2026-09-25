//! The Settings and Keybinds pages, and the GUI config load behind them. The
//! load is a cancellable background task: config parsing never runs on the UI
//! thread, and a superseded load cannot overwrite a newer one.

use super::{Page, accent};
use crate::{HerdrWindow, config::Config};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn watch_gui_config(&mut self, cx: &mut Context<Self>) {
        let Ok(path) = Config::local_path() else {
            return;
        };
        let executor = cx.background_executor().clone();
        self.config_watch = Some(cx.spawn(async move |this, cx| {
            let mut watch = crate::config::watch::Watch::default();
            let mut pending = None;
            loop {
                let path = path.clone();
                let sample = executor
                    .spawn(async move { crate::config::watch::fingerprint(&path) })
                    .await;
                let updated = this.update(cx, |this, cx| {
                    if let Some((sample, revision)) = pending
                        && this.config_load_revision != revision
                    {
                        watch.accept(sample);
                        pending = None;
                    }
                    if watch.observe(sample)
                        && this.config_load.is_none()
                        && this.menu.page != Some(Page::Themes)
                        && !this.theme_save_in_flight()
                    {
                        this.load_gui_config(cx);
                        pending = Some((sample, this.config_load_revision));
                    }
                });
                if updated.is_err() {
                    break;
                }
                executor.timer(std::time::Duration::from_millis(250)).await;
            }
        }));
    }

    pub(crate) fn open_keybinds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Keybinds);
        self.menu.keybinds_scroll.set_offset(Point::default());
        let search = cx.new(crate::search_input::SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder("Search shortcuts...", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        self.menu._keybinds_subscription = Some(cx.subscribe(
            &search,
            |this, _, _: &crate::search_input::Changed, cx| {
                this.menu.keybinds_scroll.set_offset(Point::default());
                cx.notify();
            },
        ));
        self.menu.keybinds_search = Some(search);
    }

    pub(crate) fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Preferences);
        self.menu.preferences_scroll.set_offset(Point::default());
    }

    pub(crate) fn reload_gui_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_save_in_flight() {
            return;
        }
        self.load_gui_config(cx);
        self.dismiss_menu(window, cx);
    }

    pub(crate) fn load_gui_config(&mut self, cx: &mut Context<Self>) {
        // Enumerating installed families is slow, so it rides the same
        // background load as parsing rather than the UI thread.
        let text_system = cx.text_system().clone();
        self.load_gui_config_with(
            move || {
                let mut config = Config::load()?;
                config.resolve_font_fallbacks(|| text_system.all_font_names());
                let theme = config.theme()?;
                Ok((config, theme))
            },
            cx,
        );
    }

    pub(super) fn load_gui_config_with(
        &mut self,
        load: impl FnOnce() -> crate::Result<(Config, crate::config::Theme)> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.config_load.is_some() {
            return;
        }
        let load = cx.background_executor().spawn(async move { load() });
        self.config_load = Some(cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| {
                this.config_load = None;
                this.config_load_revision = this.config_load_revision.wrapping_add(1);
                // Apply a coherent pair only after both have loaded successfully.
                match loaded {
                    Ok((config, theme)) => {
                        cx.set_global(crate::app::InitialAppearance {
                            config: config.clone(),
                            theme: theme.clone(),
                            error: None,
                        });
                        if config.keybindings != this.config.keybindings {
                            crate::actions::rebind_keys(cx);
                        }
                        if !this.config.notifications.enabled && config.notifications.enabled {
                            let cutoff = std::time::Instant::now();
                            for endpoint in &mut this.endpoints {
                                endpoint.toasts.enabled_since = Some(cutoff);
                            }
                        }
                        // Replacing the config also discards any session font
                        // adjustment, so the baseline follows the file again.
                        this.configured_terminal_size = config.terminal.size;
                        this.config = config;
                        this.tick_toasts(
                            this.menu.page.is_some() || this.toasts_hidden,
                            std::time::Instant::now(),
                        );
                        this.theme = theme;
                        crate::log_window::set_appearance(&this.config, &this.theme, cx);
                        this.wheel = Default::default();
                        this.last_queued_options = None;
                        this.local_error = None;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Could not load GUI config; keeping current settings");
                        this.local_error = Some(format!("Load GUI config: {error}"));
                    }
                }
                // A config another build wrote, such as a setting this version
                // does not know, must not sign GitHub out: restore the saved
                // credential under the settings already in effect.
                if this.avatars.is_some() && this.menu.github.initialize(&this.config) {
                    this.menu.pr_cache.clear();
                    this.menu.pr.clear();
                    this.menu.pr_connection = None;
                }
                cx.notify();
            });
        }));
    }
}

impl HerdrWindow {
    pub(super) fn render_keybinds(&self, cx: &mut Context<Self>) -> Div {
        use crate::controls::{COMMANDS, Command};

        let theme = &self.theme;
        let font = &self.config.ui;
        let query = self
            .menu
            .keybinds_search
            .as_ref()
            .map(|search| search.read(cx).text())
            .unwrap_or("");
        let accent = accent(theme);
        let mut body = div()
            .id("keybinds-body")
            .debug_selector(|| "keybinds-body".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.menu.keybinds_scroll)
            .px(px(16.))
            .py(px(8.));
        let mut groups = [
            ("WORKSPACES & PANES", Vec::new()),
            ("NAVIGATION", Vec::new()),
            ("APPLICATION", vec![(vec!["cmd-v"], "Paste into terminal")]),
        ];
        for info in COMMANDS {
            let keys: Vec<&str> = self
                .config
                .keybindings
                .shortcuts(info.command)
                .iter()
                .map(String::as_str)
                .collect();
            if keys.is_empty() {
                continue;
            }
            let group = match info.command {
                Command::Workspace
                | Command::NewWorktree
                | Command::Tab
                | Command::SplitRight
                | Command::SplitDown
                | Command::Zoom
                | Command::ClearPane
                | Command::ClosePane
                | Command::CloseTab => 0,
                Command::NextTab
                | Command::PreviousTab
                | Command::FocusLeft
                | Command::FocusRight
                | Command::FocusUp
                | Command::FocusDown
                | Command::NextPane
                | Command::PreviousPane
                | Command::TabNumber(_)
                | Command::WorkspacePicker => 1,
                Command::NewWindow
                | Command::ToggleSidebar
                | Command::CollapseSidebar
                | Command::IncreaseFontSize
                | Command::DecreaseFontSize
                | Command::ResetFontSize
                | Command::Settings
                | Command::Keybinds
                | Command::Themes
                | Command::Palette
                | Command::Reconnect
                | Command::Quit
                | Command::Logs
                | Command::About => 2,
                Command::OpenNotificationTarget => 1,
            };
            groups[group].1.push((keys, info.label));
        }
        let total: usize = groups.iter().map(|(_, shortcuts)| shortcuts.len()).sum();
        let mut count = 0;
        for (section, shortcuts) in groups {
            let shortcuts: Vec<_> = shortcuts
                .into_iter()
                .filter(|(keys, description)| {
                    keys.iter()
                        .any(|keys| shortcut_matches(query, keys, description, section))
                })
                .collect();
            if shortcuts.is_empty() {
                continue;
            }
            count += shortcuts.len();
            body = body.child(
                div()
                    .pt(px(12.))
                    .pb(px(6.))
                    .text_size(px(font.size * 0.85))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(accent)
                    .child(section),
            );
            for (keys, description) in shortcuts {
                body = body.child(
                    div()
                        .debug_selector(|| format!("shortcut-{description}"))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .py(px(7.))
                        .border_b_1()
                        .border_color(rgb(theme.active))
                        .child(
                            div()
                                .debug_selector(|| format!("keys-{description}"))
                                .w(relative(0.45))
                                .flex_none()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.))
                                .children(keys.into_iter().map(|keys| {
                                    div().flex().flex_wrap().gap(px(4.)).children(
                                        keycaps(keys).map(|key| {
                                            div()
                                                .flex_none()
                                                .px(px(6.))
                                                .py(px(2.))
                                                .rounded(px(crate::config::corners::SMALL))
                                                .border_1()
                                                .border_color(rgb(theme.active))
                                                .bg(rgb(theme.background))
                                                .text_size(px(font.size * 0.9))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(key)
                                        }),
                                    )
                                })),
                        )
                        .child(
                            div()
                                .debug_selector(|| format!("description-{description}"))
                                .flex_1()
                                .min_w_0()
                                .child(description),
                        ),
                );
            }
        }
        if count == 0 {
            body = body.child(
                div()
                    .debug_selector(|| "keybinds-empty".into())
                    .py(px(20.))
                    .text_color(rgb(theme.muted))
                    .child("No matching shortcuts. Try an action name or key combination."),
            );
        }
        body = body.child(
            div()
                .py(px(14.))
                .text_color(rgb(theme.muted))
                .child("Native GUI shortcuts only. Terminal applications and daemon/TUI keybindings keep their own shortcuts."),
        );
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .debug_selector(|| "keybinds-header".into())
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap(px(12.))
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .w(px(3.))
                            .h(px(font.size * 2.5))
                            .rounded_full()
                            .bg(accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Keyboard Shortcuts"),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme.muted))
                                    .child("Your Herdr quick reference"),
                            ),
                    )
                    .child(
                        div()
                            .id("menu-close")
                            .debug_selector(|| "keybinds-close".into())
                            .flex_none()
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(|style| {
                                style
                                    .bg(rgb(theme.active))
                                    .text_color(rgb(theme.foreground))
                            })
                            .child("Close")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "keybinds-search-area".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(8.))
                    .when_some(self.menu.keybinds_search.clone(), |area, search| {
                        area.child(search)
                    })
                    .child(
                        div()
                            .debug_selector(|| "keybinds-count".into())
                            .pt(px(4.))
                            .text_color(rgb(theme.muted))
                            .child(format!("{count} of {total} shortcuts")),
                    ),
            )
            .child(body)
            .child(
                div()
                    .debug_selector(|| "keybinds-footer".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child("Esc to close  /  click outside to dismiss"),
            )
    }
}

/// The keycaps of one keystroke, capitalized for display. `cmd--` splits into
/// `cmd` and a `-` key rather than an empty cap.
fn keycaps(keystroke: &str) -> impl Iterator<Item = String> + '_ {
    let (modifiers, key) = match keystroke.strip_suffix("--") {
        Some(modifiers) => (modifiers, "-"),
        None => keystroke.rsplit_once('-').unwrap_or(("", keystroke)),
    };
    modifiers
        .split('-')
        .filter(|modifier| !modifier.is_empty())
        .chain(std::iter::once(key))
        .map(|key| {
            let mut chars = key.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase())
                .into_iter()
                .chain(chars)
                .collect()
        })
}

fn shortcut_matches(query: &str, keys: &str, description: &str, section: &str) -> bool {
    let query = query.to_lowercase().replace(['-', '+'], " ");
    if query
        .split_whitespace()
        .next()
        .is_some_and(|token| matches!(token, "cmd" | "ctrl" | "alt" | "shift"))
    {
        // A key combination should match keycaps, not letters in an action's name.
        return query
            .split_whitespace()
            .all(|token| keys.split('-').any(|key| key == token));
    }
    let text = format!("{keys} {description} {section}")
        .to_lowercase()
        .replace('-', " ");
    query.split_whitespace().all(|token| text.contains(token))
}

#[cfg(test)]
mod tests {
    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn enabling_does_not_replay_undrained_disabled_ingress(cx: &mut gpui::TestAppContext) {
        use crate::{config::Config, notifications::tests::notification, state::ConnectionStatus};
        use herdr_client::{
            ClientEvent,
            protocol::{SemanticNotificationKind, ServerMessage},
        };
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let inbox = view.update(cx, |view, _| {
            // This fixture has no transport; polling must not start one.
            view.endpoints[0].enabled = false;
            view.endpoints[0].connection.inbox.clone()
        });
        let mut wire = notification("disabled ingress");
        wire.kind = SemanticNotificationKind::Custom;
        for _ in 0..2 {
            view.update(cx, |view, cx| {
                view.load_gui_config_with(|| Ok((Config::default(), Default::default())), cx)
            });
            cx.run_until_parked();
            {
                let mut state = inbox.lock().unwrap();
                state.status = ConnectionStatus::Connected;
                state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                    wire.clone(),
                )));
            }
            // Enabling cannot drain this inbox; the arrival fence must survive until later polling.
            let held = inbox.lock().unwrap();
            view.update(cx, |view, cx| {
                view.load_gui_config_with(
                    || {
                        let mut config = Config::default();
                        config.notifications.enabled = true;
                        config.notifications.delay_seconds = 0;
                        Ok((config, Default::default()))
                    },
                    cx,
                )
            });
            cx.run_until_parked();
            assert_eq!(held.notifications.len(), 1);
            view.read_with(cx, |view, _| assert!(view.config.notifications.enabled));
            drop(held);
            view.update(cx, |view, cx| {
                view.poll_endpoints(cx);
                assert!(view.endpoints[0].toasts.entries.is_empty());
            });
            let mut fresh = wire.clone();
            fresh.title = "enabled ingress".into();
            inbox
                .lock()
                .unwrap()
                .apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                    fresh,
                )));
            view.update(cx, |view, cx| {
                view.poll_endpoints(cx);
                assert_eq!(view.endpoints[0].toasts.entries.len(), 1);
                assert_eq!(
                    view.endpoints[0].toasts.entries[0].1.title,
                    "enabled ingress"
                );
                assert!(view.endpoints[0].toasts.entries[0].1.visible);
            });
        }
    }

    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn notification_reload_retimes_pending_clears_disabled_and_keeps_failed_settings(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::{
            config::{Config, NotificationConfig},
            notifications::{Notice, tests::notification},
        };
        use std::time::Instant;
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, _| {
            view.config.terminal.size = 24.;
            view.config.notifications = NotificationConfig {
                enabled: true,
                delay_seconds: 3600,
                ..Default::default()
            };
            view.endpoints[0]
                .toasts
                .receive([Notice::new(notification("pending"), Instant::now())]);
            view.tick_toasts(false, Instant::now());
            assert!(!view.endpoints[0].toasts.entries[0].1.visible);
        });
        view.update(cx, |view, cx| {
            view.load_gui_config_with(
                || {
                    let mut config = Config::default();
                    config.notifications.enabled = true;
                    config.notifications.delay_seconds = 0;
                    config.notifications.position =
                        herdr_client::protocol::ToastHerdrPosition::TopRight;
                    config.terminal.size = 18.;
                    config.layout.sidebar_gap = 16.;
                    config.layout.mode =
                        crate::config::LayoutMode::from(crate::config::Density::Compact);
                    let theme = config.theme()?;
                    Ok((config, theme))
                },
                cx,
            )
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert!(view.endpoints[0].toasts.entries[0].1.visible);
            assert_eq!(view.config.notifications.delay_seconds, 0);
            assert_eq!(view.config.terminal.size, 18.);
            assert_eq!(view.configured_terminal_size, 18.);
            assert_eq!(view.config.layout.sidebar_gap, 16.);
            assert_eq!(
                view.config.layout.mode,
                crate::config::LayoutMode::from(crate::config::Density::Compact)
            );
            view.set_terminal_font_size(20., cx);
            view.load_gui_config_with(|| Err(crate::Error::MissingHome), cx);
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert!(view.config.notifications.enabled);
            assert!(view.endpoints[0].toasts.entries[0].1.visible);
            assert_eq!(view.config.terminal.size, 20.);
            assert_eq!(view.configured_terminal_size, 18.);
            assert_eq!(view.config.layout.sidebar_gap, 16.);
            assert_eq!(
                view.config.layout.mode,
                crate::config::LayoutMode::from(crate::config::Density::Compact)
            );
            view.load_gui_config_with(|| Ok((Config::default(), Default::default())), cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.config.notifications.enabled);
            assert!(view.endpoints[0].toasts.entries.is_empty());
            assert_eq!(view.config.terminal.size, Config::default().terminal.size);
            assert_eq!(view.configured_terminal_size, view.config.terminal.size);
            assert_eq!(view.config.layout, Config::default().layout);
        });
    }

    #[gpui::test]
    fn config_reload_toggles_tab_flags_and_preserves_them_on_failure(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        for (confirm_close_tab, show_agents) in
            [(false, true), (true, false), (false, false), (true, true)]
        {
            view.update(cx, |view, cx| {
                view.load_gui_config_with(
                    move || {
                        let config = crate::config::Config {
                            confirm_close_tab,
                            show_agents,
                            ..Default::default()
                        };
                        let theme = config.theme()?;
                        Ok((config, theme))
                    },
                    cx,
                );
            });
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(
                    (view.config.confirm_close_tab, view.config.show_agents),
                    (confirm_close_tab, show_agents)
                );
                assert!(view.config_load.is_none());
                assert!(view.local_error.is_none());
            });
            for error in [crate::Error::MissingHome, crate::Error::EmptyTheme] {
                view.update(cx, |view, cx| {
                    view.load_gui_config_with(move || Err(error), cx);
                });
                cx.run_until_parked();
                view.read_with(cx, |view, _| {
                    assert_eq!(
                        (view.config.confirm_close_tab, view.config.show_agents),
                        (confirm_close_tab, show_agents)
                    );
                    assert!(view.config_load.is_none());
                    assert!(view.local_error.is_some());
                });
            }
        }
    }

    #[gpui::test]
    fn failed_config_load_still_restores_the_saved_github_sign_in(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.avatars = Some(crate::avatars::Avatars::new());
            view.load_gui_config_with(|| Err(crate::Error::MissingHome), cx);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.local_error.is_some());
            // The restore is queued under the settings already in effect; the
            // next poll reads the saved credential off the UI thread.
            assert!(view.menu.github.loading_profile());
            assert_eq!(
                view.menu.github.store(),
                crate::github::Store::select(&view.config)
            );
        });
    }

    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn config_load_is_coherent_bounded_and_cancellable(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.load_gui_config_with(
                || {
                    let config = crate::config::Config {
                        theme: "Nord".into(),
                        ..Default::default()
                    };
                    let theme = config.theme()?;
                    Ok((config, theme))
                },
                cx,
            );
            view.load_gui_config_with(|| panic!("only one config load at a time"), cx);
            assert_eq!(view.config.theme, "Default");
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.config.theme, "Nord");
            assert_eq!(view.theme, view.config.theme().unwrap());
            assert!(view.config_load.is_none());
            assert_eq!(view.config_load_revision, 1);
            view.load_gui_config_with(|| Err(crate::Error::EmptyTheme), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.config.theme, "Nord");
                assert_eq!(view.theme, view.config.theme().unwrap());
                assert_eq!(view.config_load_revision, 2);
                assert!(
                    view.local_error
                        .as_deref()
                        .unwrap()
                        .contains("theme must not be empty")
                );
                view.load_gui_config_with(|| Ok((Default::default(), Default::default())), cx);
                view.open_theme_picker(window, cx);
                assert!(view.config_load.is_none());
            });
        });
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert_eq!(view.config.theme, "Nord");
            assert_eq!(
                view.config_load_revision, 2,
                "cancelled loads are not acknowledged"
            );
        });
    }

    #[test]
    fn shortcut_search_matches_labels_keys_and_sections() {
        for query in ["", "pane close", "CMD+W", "cmd-w", "workspaces"] {
            assert!(super::shortcut_matches(
                query,
                "cmd-w",
                "Close Pane",
                "WORKSPACES & PANES"
            ));
        }
        assert!(!super::shortcut_matches(
            "zoom",
            "cmd-w",
            "Close Pane",
            "WORKSPACES & PANES"
        ));
        assert!(super::shortcut_matches(
            "cmd shift p",
            "cmd-shift-p",
            "Command Palette",
            "APPLICATION"
        ));
        assert!(!super::shortcut_matches(
            "cmd+p",
            "cmd-d",
            "Split Right",
            "WORKSPACES & PANES"
        ));
    }

    #[test]
    fn keycaps_split_modifiers_from_the_key() {
        let caps = |keystroke| super::keycaps(keystroke).collect::<Vec<_>>();
        assert_eq!(caps("cmd-shift-t"), ["Cmd", "Shift", "T"]);
        assert_eq!(caps("cmd--"), ["Cmd", "-"]);
        assert_eq!(caps("cmd-+"), ["Cmd", "+"]);
        assert_eq!(caps("f5"), ["F5"]);
    }

    /// A saved `[keybindings]` change must reach the live keymap, the palette,
    /// and the keybindings page without restarting, and keep the console keys.
    #[gpui::test]
    #[allow(clippy::unwrap_used)]
    fn config_reload_rebinds_the_keymap(cx: &mut gpui::TestAppContext) {
        use crate::{
            Command, RunCommand,
            config::Config,
            keymap::{Binding, Keymap},
        };
        use gpui::Keystroke;

        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.update(|_, cx| crate::bind_keys(cx));
        let runs = |keystroke: &str, command: Command, cx: &mut gpui::VisualTestContext| {
            cx.update(|_, cx| {
                cx.key_bindings()
                    .borrow()
                    .all_bindings_for_input(&[Keystroke::parse(keystroke).unwrap()])
                    .iter()
                    .any(|binding| binding.action().partial_eq(&RunCommand { command }))
            })
        };
        assert!(runs("cmd-t", Command::Tab, cx));
        assert!(!runs("cmd-n", Command::Tab, cx));
        assert!(runs("cmd-shift-n", Command::Workspace, cx));

        view.update(cx, |view, cx| {
            view.load_gui_config_with(
                || {
                    let overrides = [
                        ("new_workspace", Binding::One("cmd-t".into())),
                        ("toggle_sidebar", Binding::Many(Vec::new())),
                    ]
                    .into_iter()
                    .map(|(name, binding)| (name.to_owned(), binding))
                    .collect();
                    let config = Config {
                        keybindings: Keymap::with_overrides(&overrides)?,
                        ..Config::default()
                    };
                    Ok((config, Default::default()))
                },
                cx,
            )
        });
        cx.run_until_parked();
        assert!(runs("cmd-t", Command::Workspace, cx));
        assert!(!runs("cmd-t", Command::Tab, cx));
        assert!(!runs("cmd-shift-n", Command::Workspace, cx));
        assert!(!runs("cmd-b", Command::ToggleSidebar, cx));
        cx.update(|_, cx| {
            let keymap = cx.key_bindings();
            let keymap = keymap.borrow();
            let console = keymap.all_bindings_for_input(&[Keystroke::parse("cmd-l").unwrap()]);
            assert_eq!(console.len(), 1);
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.config.keybindings.primary(Command::Workspace), "cmd-t");
            assert_eq!(view.config.keybindings.primary(Command::Tab), "");
        });

        view.update(cx, |view, cx| {
            view.load_gui_config_with(|| Ok((Config::default(), Default::default())), cx)
        });
        cx.run_until_parked();
        assert!(runs("cmd-t", Command::Tab, cx));
        view.read_with(cx, |view, _| {
            assert_eq!(view.config.keybindings, Keymap::default())
        });
    }
}

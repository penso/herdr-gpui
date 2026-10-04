//! The Settings and Keybinds pages, and the GUI config load behind them. The
//! load is a cancellable background task: config parsing never runs on the UI
//! thread, and a superseded load cannot overwrite a newer one.

use super::{Page, accent};
use crate::{
    HerdrWindow,
    config::{Config, FONT_SIZE_RANGE, FONT_SIZE_STEP, FontFace},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn reload_notification_config(&mut self, cx: &mut Context<Self>) {
        self.sound.reload();
        use crate::config::NotificationDelivery::Off;
        let was_off = self.config.notifications.delivery() == Off;
        if let Some(shared) = &self.settings.shared {
            self.config.apply_shared_notifications(shared);
        }
        let now = std::time::Instant::now();
        for endpoint in &mut self.endpoints {
            // Turning delivery on, in-app or system, never replays the backlog.
            if was_off && self.config.notifications.delivery() != Off {
                endpoint.toasts.enabled_since = Some(now);
            }
        }
        self.tick_toasts(self.menu.page.is_some() || self.toasts_hidden, now);
        cx.notify();
    }

    /// Reloads when the GUI overrides change, or the daemon's config whose
    /// `[keys]`, clipboard toast, and `[ui.sidebar]` rows the GUI also honors.
    pub(crate) fn watch_gui_config(&mut self, cx: &mut Context<Self>) {
        let Ok(path) = Config::local_path() else {
            return;
        };
        let daemon = crate::config::daemon_config_path(|key| std::env::var_os(key));
        let executor = cx.background_executor().clone();
        self.config_watch = Some(cx.spawn(async move |this, cx| {
            let mut watch = crate::config::watch::Watch::default();
            let mut pending = None;
            loop {
                let (path, daemon) = (path.clone(), daemon.clone());
                let sample = executor
                    .spawn(async move {
                        use crate::config::watch::fingerprint;
                        [fingerprint(&path), fingerprint(&daemon)]
                    })
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
                        && this.settings.task.is_none()
                        && !this.font_size_saves.is_busy()
                        && !matches!(this.menu.page, Some(Page::Themes | Page::Fonts))
                        && !this.theme_save_in_flight()
                        && !this.native_settings_save_in_flight()
                    {
                        this.load_gui_config(cx);
                        this.load_shared_settings(cx);
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
        if self.theme_save_in_flight() {
            return;
        }
        self.dismiss_menu(window, cx);
        crate::settings_window::open(cx.weak_entity(), cx);
    }

    // Existing modal font/theme pickers still return to their own preferences page.
    #[cfg(any(test, all(feature = "integration-test", target_os = "macos")))]
    pub(crate) fn open_preferences_fixture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Preferences);
        self.menu.preferences_scroll.set_offset(Point::default());
        self.load_shared_settings(cx);
        self.select_settings_tab(self.settings.tab, window, cx);
    }

    pub(crate) fn change_font_size(
        &mut self,
        face: FontFace,
        direction: f32,
        cx: &mut Context<Self>,
    ) {
        let current = face.size(&self.config);
        let size = (current + direction * FONT_SIZE_STEP)
            .clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end());
        self.set_font_size(face, size, cx);
    }

    pub(crate) fn reload_gui_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_save_in_flight() || self.native_settings_save_in_flight() {
            return;
        }
        self.load_gui_config(cx);
        self.dismiss_menu(window, cx);
    }

    pub(crate) fn load_gui_config(&mut self, cx: &mut Context<Self>) {
        if self.native_settings_save_in_flight() {
            return;
        }
        // Enumerating installed families is slow, so it rides the same
        // background load as parsing rather than the UI thread.
        let text_system = cx.text_system().clone();
        self.load_gui_config_with(
            move || {
                let mut config = Config::load()?;
                config.resolve_font_fallbacks(|| text_system.all_font_names());
                // Follow Herdr is resolved from the latest prepared snapshot on completion.
                let theme = if config.theme == "Follow Herdr" {
                    Default::default()
                } else {
                    config.theme()?
                };
                Ok((config, theme))
            },
            cx,
        );
    }

    pub(crate) fn load_gui_config_with(
        &mut self,
        load: impl FnOnce() -> crate::Result<(Config, crate::config::Theme)> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.config_load.is_some() || self.font_size_saves.is_busy() {
            return;
        }
        let theme_revision = crate::settings_window::theme_load_revision(cx);
        let layout_revision = crate::settings_window::layout_load_revision(cx);
        let load = cx.background_executor().spawn(async move { load() });
        self.config_load = Some(cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| {
                this.config_load = None;
                let native_reload = std::mem::take(&mut this.settings.native_reloading);
                this.config_load_revision = this.config_load_revision.wrapping_add(1);
                // Apply a coherent pair only after both have loaded successfully.
                match loaded {
                    Ok((mut config, mut theme)) => {
                        crate::settings_window::apply_loaded_layout(&mut config, layout_revision, cx);
                        crate::settings_window::apply_loaded_theme(&mut config, &mut theme, theme_revision, cx);
                        if let Some(shared) = &this.settings.shared {
                            config.apply_shared_notifications(shared);
                        }
                        // A reload discards session zoom; queued Settings edits
                        // remain visible but do not become the saved baseline yet.
                        this.configured_terminal_size = config.terminal.size;
                        cx.set_global(crate::app::InitialAppearance {
                            config: config.clone(),
                            theme: theme.clone(),
                            error: None,
                        });
                        if config.keybindings != this.config.keybindings {
                            crate::actions::rebind_keys(cx);
                        } else if config.layout.mode != this.config.layout.mode {
                            // The View menu checks the layout in use.
                            crate::menus::install(cx);
                        }
                        if this.config.notifications.delivery() == crate::config::NotificationDelivery::Off
                            && config.notifications.delivery() != crate::config::NotificationDelivery::Off
                        {
                            let cutoff = std::time::Instant::now();
                            for endpoint in &mut this.endpoints {
                                endpoint.toasts.enabled_since = Some(cutoff);
                            }
                        }
                        this.config = config.clone();
                        if this.config.theme != "Follow Herdr" {
                            this.theme = theme;
                        }
                        this.apply_shared_theme(cx);
                        crate::settings_window::apply_loaded_theme(&mut this.config, &mut this.theme, theme_revision, cx);
                        cx.set_global(crate::app::InitialAppearance {
                            config: this.config.clone(),
                            theme: this.theme.clone(),
                            error: None,
                        });
                        this.font_size_saves.apply_pending(&mut config);
                        this.config = config;
                        this.gui_config_diagnostic.sync(this.config.diagnostic().as_deref());
                        crate::settings_window::apply_loaded_theme(&mut this.config, &mut this.theme, theme_revision, cx);
                        this.tick_toasts(
                            this.menu.page.is_some() || this.toasts_hidden,
                            std::time::Instant::now(),
                        );
                        if native_reload {
                            this.settings.native_status =
                                Some("GUI config saved and applied".into());
                        }
                        crate::log_window::set_appearance(&this.config, &this.theme, cx);
                        this.wheel = Default::default();
                        this.last_queued_options = None;
                        this.local_error = None;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Could not load GUI config; keeping current settings");
                        let message = format!("Load GUI config: {error}");
                        if native_reload {
                            this.settings.native_status =
                                Some("GUI config saved; appearance reload failed".into());
                            this.settings.native_error = Some(message.clone());
                        }
                        this.local_error = Some(message);
                    }
                }
                // A config another build wrote, such as a setting this version
                // does not know, must not sign GitHub out: restore the saved
                // credential under the settings already in effect.
                let mut reloaded = false;
                if this.avatars.is_some() {
                    reloaded = this.menu.github.initialize(&this.config);
                    for auth in this.menu.github_hosts.values_mut() {
                        reloaded |= auth.initialize(&this.config);
                    }
                }
                if reloaded {
                    this.menu.pr_cache.clear();
                    this.menu.pr.clear();
                    this.menu.pr_connection = None;
                }
                this.flush_font_sizes(cx);
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
            let mut keys: Vec<&str> = self.keymap().shortcuts(info.command).collect();
            if info.command == Command::Palette && self.config.palette.double_shift {
                keys.push("shift shift");
            }
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
                | Command::Find
                | Command::CopyMode
                | Command::EditScrollback
                | Command::ClosePane
                | Command::CloseTab
                | Command::NewBrowserTab
                | Command::SplitEditor
                | Command::MoveTabPrevious
                | Command::MoveTabNext
                | Command::RenameTab
                | Command::SwapLeft
                | Command::SwapRight
                | Command::SwapUp
                | Command::SwapDown
                | Command::ResizeLeft
                | Command::ResizeRight
                | Command::ResizeUp
                | Command::ResizeDown
                | Command::ResizeMode
                | Command::RenamePane
                | Command::RenameWorkspace
                | Command::CloseWorkspace => 0,
                Command::NextTab
                | Command::PreviousTab
                | Command::FocusLeft
                | Command::FocusRight
                | Command::FocusUp
                | Command::FocusDown
                | Command::NextPane
                | Command::PreviousPane
                | Command::TabNumber(_)
                | Command::WorkspacePicker
                | Command::LastPane
                | Command::PreviousWorkspace
                | Command::NextWorkspace
                | Command::WorkspaceNumber(_)
                | Command::PreviousAgent
                | Command::NextAgent
                | Command::AgentNumber(_) => 1,
                Command::NewWindow
                | Command::ToggleSidebar
                | Command::IncreaseFontSize
                | Command::DecreaseFontSize
                | Command::ResetFontSize
                | Command::Settings
                | Command::Keybinds
                | Command::Sessions
                | Command::Themes
                | Command::Palette
                | Command::Reconnect
                | Command::Quit
                | Command::Logs
                | Command::About
                | Command::InstallBrowserSkill
                | Command::ReloadConfig => 2,
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
                .child("Includes the prefix chords from Herdr's [keys] in config.toml. Daemon actions with no GUI command, and terminal applications, keep their own shortcuts."),
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

/// The keycaps of a shortcut, capitalized for display, a prefix chord's
/// keystrokes in turn. `cmd--` splits into `cmd` and a `-` key rather than an
/// empty cap, as does a chord's bare `-`.
fn keycaps(shortcut: &str) -> impl Iterator<Item = String> + '_ {
    shortcut
        .split(' ')
        .flat_map(|keystroke| {
            let (modifiers, key) = match keystroke.strip_suffix('-') {
                Some(modifiers) if modifiers.is_empty() || modifiers.ends_with('-') => {
                    (modifiers, "-")
                }
                _ => keystroke.rsplit_once('-').unwrap_or(("", keystroke)),
            };
            modifiers
                .split('-')
                .filter(|modifier| !modifier.is_empty())
                .chain(std::iter::once(key))
        })
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
            .all(|token| keys.split(['-', ' ']).any(|key| key == token));
    }
    let text = format!("{keys} {description} {section}")
        .to_lowercase()
        .replace('-', " ");
    query.split_whitespace().all(|token| text.contains(token))
}

#[cfg(test)]
mod tests;

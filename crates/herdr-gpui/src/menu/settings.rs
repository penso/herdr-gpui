//! The Settings and Keybinds pages, and the GUI config load behind them. The
//! load is a cancellable background task: config parsing never runs on the UI
//! thread, and a superseded load cannot overwrite a newer one.

// i18n: migrated
use super::{Page, accent};
use crate::i18n::fl;
use crate::{HerdrWindow, config::Config, keymap::Reach};
use gpui::{prelude::*, *};
use herdr_client::protocol::ClientShellCommand;
use std::borrow::Cow;

/// The semantic paste row, which no keymap command holds.
fn paste_label() -> String {
    fl!("menu-settings-paste")
}

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
        self.tick_toasts(self.menu.page.is_some() || self.toasts_hidden, now, cx);
        cx.notify();
    }

    /// Reloads when the GUI overrides change, or the daemon's config whose
    /// `[keys]`, clipboard toast, and `[ui.sidebar]` rows the GUI also honors.
    /// Reads the theme again when a theme file changes in place.
    pub(crate) fn watch_gui_config(&mut self, cx: &mut Context<Self>) {
        let Ok(path) = Config::local_path() else {
            return;
        };
        let daemon = crate::config::daemon_config_path(herdr_client::local_var);
        let executor = cx.background_executor().clone();
        let mut theme = self.config.theme.clone();
        self.config_watch = Some(cx.spawn(async move |this, cx| {
            let mut watch = crate::config::watch::Watch::default();
            let mut theme_watch = crate::config::watch::ThemeWatch::default();
            let mut pending = None;
            loop {
                let (path, daemon) = (path.clone(), daemon.clone());
                let (sample, theme_sample, sampled) = executor
                    .spawn(async move {
                        use crate::config::watch::{fingerprint, fingerprint_all};
                        let theme_sample = fingerprint_all(Config::theme_files(&theme));
                        (
                            [fingerprint(&path), fingerprint(&daemon)],
                            theme_sample,
                            theme,
                        )
                    })
                    .await;
                theme = sampled;
                let updated = this.update(cx, |this, cx| {
                    if theme_watch.observe(&theme, theme_sample, &this.config.theme)
                        && this.config_load.is_none()
                        && this.reload_theme(cx)
                    {
                        theme_watch.accept(theme_sample);
                    }
                    theme.clone_from(&this.config.theme);
                    if let Some((sample, revision)) = pending
                        && this.config_load_revision != revision
                    {
                        watch.accept(sample);
                        pending = None;
                    }
                    if watch.observe(sample)
                        && this.config_load.is_none()
                        && this.settings.task.is_none()
                        && !matches!(this.menu.page, Some(Page::Themes))
                        && !this.theme_save_in_flight()
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
            input.set_placeholder_with(|| fl!("menu-settings-search"), cx);
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
        let light = crate::app::light_appearance(cx);
        let text_system = cx.text_system().clone();
        self.load_gui_config_with(
            move || {
                let mut config = Config::load()?;
                config.resolve_fonts(|| text_system.all_font_names());
                // Follow Herdr is resolved from the latest prepared snapshot on completion.
                // i18n: data
                let theme = if config.theme == "Follow Herdr" {
                    Default::default()
                } else {
                    config.theme(light)?
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
        if self.config_load.is_some() {
            return;
        }
        let theme_revision = crate::settings_window::theme_load_revision(cx);
        let layout_revision = crate::settings_window::layout_load_revision(cx);
        let sidebar_revision = crate::settings_window::sidebar_load_revision(cx);
        // Loaders resolve the theme for the appearance when they start.
        let light = crate::app::light_appearance(cx);
        let load = cx.background_executor().spawn(async move { load() });
        self.config_load = Some(cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| {
                this.config_load = None;
                this.config_load_revision = this.config_load_revision.wrapping_add(1);
                // Apply a coherent pair only after both have loaded successfully.
                match loaded {
                    Ok((mut config, mut theme)) => {
                        crate::settings_window::apply_loaded_layout(&mut config, layout_revision, cx);
                        crate::settings_window::apply_loaded_sidebar(&mut config, sidebar_revision, cx);
                        crate::settings_window::apply_loaded_theme(&mut config, &mut theme, theme_revision, cx);
                        if let Some(shared) = &this.settings.shared {
                            config.apply_shared_notifications(shared);
                        }
                        // A reload discards session zoom.
                        this.configured_terminal_size = config.terminal.size;
                        cx.set_global(crate::app::InitialAppearance {
                            config: config.clone(),
                            theme: theme.clone(),
                            error: None,
                        });
                        crate::caffeine::set_scope(config.status_bar.keep_awake_scope, cx);
                        crate::i18n::follow_config(&config.language, cx);
                        let menus_changed = crate::menus::changed(&this.config, &config);
                        if config.keybindings != this.config.keybindings {
                            crate::actions::rebind_keys(cx);
                        } else if menus_changed {
                            crate::menus::install(cx);
                        }
                        if menus_changed {
                            this.close_stale_application_menu(cx);
                        }
                        if this.config.notifications.delivery() == crate::config::NotificationDelivery::Off
                            && config.notifications.delivery() != crate::config::NotificationDelivery::Off
                        {
                            let cutoff = std::time::Instant::now();
                            for endpoint in &mut this.endpoints {
                                endpoint.toasts.enabled_since = Some(cutoff);
                            }
                        }
                        if config.status_bar.show != this.config.status_bar.show {
                            this.status_bar_visible = config.status_bar.show;
                        }
                        this.config = config.clone();
                        // i18n: data
                        if this.config.theme != "Follow Herdr" {
                            this.theme = theme;
                        }
                        this.apply_shared_theme(cx);
                        this.theme_light = light;
                        if light != crate::app::light_appearance(cx) {
                            this.apply_system_theme(cx);
                        }
                        crate::settings_window::apply_loaded_theme(&mut this.config, &mut this.theme, theme_revision, cx);
                        cx.set_global(crate::app::InitialAppearance {
                            config: this.config.clone(),
                            theme: this.theme.clone(),
                            error: None,
                        });
                        this.config = config;
                        this.sound.set_volume(this.config.sound.volume);
                        this.gui_config_diagnostic.sync(this.config.diagnostic().as_deref());
                        crate::settings_window::apply_loaded_theme(&mut this.config, &mut this.theme, theme_revision, cx);
                        this.tick_toasts(
                            this.menu.page.is_some() || this.toasts_hidden,
                            std::time::Instant::now(),
                            cx,
                        );
                        crate::log_window::set_appearance(&this.config, &this.theme, cx);
                        this.wheel = Default::default();
                        this.last_queued_options = None;
                        this.local_error = None;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Could not load GUI config; keeping current settings");
                        let message = fl!("menu-settings-load-failed", error = error.to_string());
                        this.local_error = Some(message);
                    }
                }
                // A config another build wrote, such as a setting this version
                // does not know, must not sign GitHub out: restore the saved
                // credential under the settings already in effect.
                let mut reloaded = false;
                if this.avatars.is_some() {
                    reloaded = this.menu.github.initialize(&this.config.github);
                    for auth in this.menu.github_hosts.values_mut() {
                        reloaded |= auth.initialize(&this.config.github);
                    }
                    for account in &mut this.menu.github_accounts {
                        reloaded |= account.auth.initialize(&this.config.github);
                    }
                    this.sync_github_accounts();
                }
                if reloaded {
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
        let editor = self.render_shortcut_editor(cx);
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
            .py(px(8.))
            .children(editor);
        let paste = paste_label();
        let mut groups = [
            (fl!("menu-settings-workspaces"), Vec::new()),
            (fl!("menu-settings-navigation"), Vec::new()),
            (
                fl!("menu-settings-application"),
                vec![(
                    vec![(Cow::Borrowed(crate::actions::EDIT_KEYS[2]), None)],
                    Cow::Owned(paste.clone()),
                    None,
                )],
            ),
            (fl!("menu-settings-commands"), Vec::new()),
        ];
        for info in COMMANDS {
            let mut keys: Vec<&str> = self.keymap().shortcuts(info.command).collect();
            if info.command == Command::Palette && self.config.palette.double_shift {
                keys.push("shift shift"); // i18n: data
            }
            let group = match info.command {
                Command::Workspace
                | Command::NewWorktree
                | Command::OpenWorktree
                | Command::RemoveWorktree
                | Command::EditWorktreeNote
                | Command::Tab
                | Command::SplitRight
                | Command::SplitDown
                | Command::Zoom
                | Command::ClearPane
                | Command::Find
                | Command::FindNext
                | Command::FindPrevious
                | Command::CopyMode
                | Command::AnnotateSelection
                | Command::EditScrollback
                | Command::ClosePane
                | Command::CloseTab
                | Command::ReopenClosedTab
                | Command::NewBrowserTab
                | Command::OpenOrchestrator
                | Command::ReviewChanges
                | Command::GitMenu
                | Command::ToggleAnnotate
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
                | Command::EqualizeSplits
                | Command::RenamePane
                | Command::MovePane
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
                | Command::GoToSymbol
                | Command::GoToFile
                | Command::WorktreeNotes
                | Command::LastPane
                | Command::Back
                | Command::Forward
                | Command::PreviousWorkspace
                | Command::NextWorkspace
                | Command::WorkspaceNumber(_)
                | Command::PreviousAgent
                | Command::NextAgent
                | Command::AgentNumber(_) => 1,
                Command::NewWindow
                | Command::ToggleSidebar
                | Command::OpenCode
                | Command::OpenPx0
                | Command::RemotePair
                | Command::ToggleStatusBar
                | Command::ToggleModPanel
                | Command::FocusNextRegion
                | Command::FocusPreviousRegion
                | Command::ToggleZenMode
                | Command::DevicesOverview
                | Command::NewCoreTerminal
                | Command::MirrorCorePane
                | Command::IncreaseFontSize
                | Command::DecreaseFontSize
                | Command::ResetFontSize
                | Command::Settings
                | Command::Keybinds
                | Command::Sessions
                | Command::Themes
                | Command::Palette
                | Command::ResumePalette
                | Command::Detach
                | Command::Reconnect
                | Command::Quit
                | Command::Logs
                | Command::About
                | Command::InstallBrowserSkill
                | Command::ToggleFullScreen
                | Command::CycleWindows
                | Command::CloseWindow
                | Command::ReloadConfig => 2,
                Command::OpenNotificationTarget => 1,
            };
            groups[group].1.push((
                keys.into_iter()
                    .map(|key| (Cow::Borrowed(key), None))
                    .collect(),
                Cow::Owned(crate::i18n::command(info)),
                Some(info),
            ));
        }
        if let Some(snapshot) = self
            .live
            .snapshot
            .as_ref()
            .filter(|_| self.live.status.is_connected())
        {
            let commands = &snapshot.commands;
            let bindings = self.keymap().custom_bindings(commands);
            groups[3]
                .1
                .extend(commands.iter().zip(bindings).enumerate().map(
                    |(index, (command, bindings))| {
                        let description = command_name(command);
                        // A row's name keys its selectors and is all that tells
                        // rows apart, so one already used by a built-in action
                        // or an earlier command is numbered. Daemon command IDs
                        // are generated per boot and mean nothing to a reader.
                        let earlier = usize::from(
                            description == paste
                                || COMMANDS
                                    .iter()
                                    .any(|info| crate::i18n::command(info) == description),
                        ) + commands[..index]
                            .iter()
                            .filter(|other| command_name(other) == description)
                            .count();
                        let description = match earlier {
                            0 => description.to_owned(),
                            earlier => format!("{description} ({})", earlier + 1),
                        };
                        (
                            bindings
                                .into_iter()
                                .map(|binding| {
                                    (Cow::Owned(binding.label), reach_note(binding.reach))
                                })
                                .collect(),
                            Cow::Owned(description),
                            None,
                        )
                    },
                ));
        }
        let total: usize = groups.iter().map(|(_, shortcuts)| shortcuts.len()).sum();
        let mut count = 0;
        for (section, shortcuts) in groups {
            let shortcuts: Vec<_> = shortcuts
                .into_iter()
                .filter(|(keys, description, _)| {
                    shortcut_matches(query, "", description, &section)
                        || keys
                            .iter()
                            .any(|(keys, _)| shortcut_matches(query, keys, description, &section))
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
            for (keys, description, info) in shortcuts {
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
                                .when(keys.is_empty(), |column| {
                                    column.child(
                                        div()
                                            .text_color(rgb(theme.muted))
                                            .child(fl!("menu-settings-no-shortcut")),
                                    )
                                })
                                .children(keys.into_iter().map(|(keys, note)| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(3.))
                                        .child(div().flex().flex_wrap().gap(px(4.)).children(
                                            keycaps(&keys).map(|key| {
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
                                        ))
                                        .when_some(note, |binding, note| {
                                            binding.child(
                                                div()
                                                    .text_size(px(font.size * 0.85))
                                                    .text_color(rgb(theme.muted))
                                                    .child(note),
                                            )
                                        })
                                })),
                        )
                        .child(
                            div()
                                .debug_selector(|| format!("description-{description}"))
                                .flex_1()
                                .min_w_0()
                                .child(description),
                        )
                        .when_some(info, |row, info| {
                            row.child(
                                div()
                                    .id(SharedString::from(format!("edit-shortcut-{}", info.name)))
                                    .debug_selector(move || format!("edit-shortcut-{}", info.name))
                                    .flex_none()
                                    .px(px(6.))
                                    .py(px(4.))
                                    .cursor_pointer()
                                    .text_color(accent)
                                    .child(fl!("menu-settings-edit"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_shortcut(info, window, cx)
                                    })),
                            )
                        }),
                );
            }
        }
        if count == 0 {
            body = body.child(
                div()
                    .debug_selector(|| "keybinds-empty".into())
                    .py(px(20.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("menu-settings-no-match")),
            );
        }
        body = body.child(
            div()
                .py(px(14.))
                .text_color(rgb(theme.subtext()))
                .child(fl!("menu-settings-note")),
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
                                    .child(fl!("menu-settings-title")),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme.muted))
                                    .child(fl!("menu-settings-subtitle")),
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
                            .child(fl!("menu-settings-close"))
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
                            .child(fl!("menu-settings-count", count = count, total = total)),
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
                    .child(fl!("menu-settings-footer")),
            )
    }
}

/// The keycaps of a shortcut, capitalized for display, a prefix chord's
/// keystrokes in turn. `cmd--` splits into `cmd` and a `-` key rather than an
/// empty cap, as does a chord's bare `-`.
/// A custom command's row name: its description, or its ID without one.
fn command_name(command: &ClientShellCommand) -> &str {
    command
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .unwrap_or(&command.command_id)
}

/// Why a custom command's binding does not run, beneath its keycaps.
fn reach_note(reach: Reach) -> Option<String> {
    Some(match reach {
        Reach::Runs => return None,
        Reach::Shadowed => fl!("menu-settings-shadowed"),
        Reach::Taken => fl!("menu-settings-taken"),
        Reach::NeedsModifier => fl!("menu-settings-needs-modifier"),
        Reach::NoPrefix => fl!("menu-settings-no-prefix"),
        Reach::OverLimit => fl!("menu-settings-over-limit", limit = 8),
        Reach::Unsupported => fl!("menu-settings-unsupported"),
    })
}

pub(super) fn keycaps(shortcut: &str) -> impl Iterator<Item = String> + '_ {
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
        let keys = keys.to_lowercase();
        return query
            .split_whitespace()
            .all(|token| keys.split(['-', '+', ' ']).any(|key| key == token));
    }
    let text = format!("{keys} {description} {section}")
        .to_lowercase()
        .replace(['-', '+'], " ");
    query.split_whitespace().all(|token| text.contains(token))
}

#[cfg(test)]
mod tests;

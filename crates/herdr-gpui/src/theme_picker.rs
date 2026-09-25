//! The theme picker: a kit `Command` in a dialog. Moving the highlight
//! previews a theme; confirming saves it; dismissing restores the theme the
//! picker opened with.

use crate::{HerdrWindow, config::Theme, menu::Page};
use gpui_kit::{
    component::{
        ActiveTheme as _, IndexPath,
        command::{Command, CommandItem, CommandState},
        dialog::Dialog,
        v_flex,
    },
    prelude::*,
    *,
};

pub(crate) struct ThemePicker {
    pub(crate) command: Entity<CommandState>,
    names: Vec<String>,
    pub(crate) filtered: Vec<String>,
    pub(crate) selected: usize,
    /// The highlight still has to be moved to `selected` once the kit list
    /// has rows to highlight.
    highlight: bool,
    error: Option<String>,
    query: String,
    discovering: bool,
    baseline: Option<Theme>,
    session: u64,
    request: u64,
    desired: Option<String>,
    loaded: Option<String>,
    accepting: bool,
    saving: bool,
    // Keep the slot across dismiss/reopen: blocking I/O cannot be cancelled by
    // dropping a GPUI task. Only its completion may release the slot.
    in_flight: Option<(u64, u64)>,
    window: AnyWindowHandle,
}

impl ThemePicker {
    fn filter(&mut self, query: &str) {
        self.query = query.to_owned();
        let query = query.trim().to_lowercase();
        self.filtered = self
            .names
            .iter()
            .filter(|name| name.to_lowercase().contains(&query))
            .cloned()
            .collect();
        self.selected = 0;
    }

    fn status(&self) -> String {
        if self.saving {
            "Saving theme... Please wait.".to_owned()
        } else if let Some(error) = &self.error {
            error.clone()
        } else if self.accepting {
            "Loading theme... Esc to cancel.".to_owned()
        } else {
            "Move the highlight to preview. Enter or click a theme to save.".to_owned()
        }
    }
}

impl HerdrWindow {
    pub(crate) fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.begin_menu(window, cx) {
            return;
        }
        // A pending reload must not replace this newer interactive appearance.
        self.config_load = None;
        let command = cx.new(|cx| CommandState::new(window, cx));
        let mut picker = if let Some(mut picker) = self.menu.themes.take() {
            picker.command = command.clone();
            picker.query.clear();
            picker
        } else {
            ThemePicker {
                command: command.clone(),
                names: Vec::new(),
                filtered: Vec::new(),
                selected: 0,
                highlight: false,
                error: None,
                query: String::new(),
                discovering: false,
                baseline: None,
                session: 0,
                request: 0,
                desired: None,
                loaded: None,
                accepting: false,
                saving: false,
                in_flight: None,
                window: window.window_handle(),
            }
        };
        picker.session += 1;
        picker.baseline = Some(self.theme.clone());
        picker.desired = None;
        picker.loaded = None;
        picker.accepting = false;
        picker.error = None;
        picker.names = Theme::BUILTIN_NAMES
            .iter()
            .map(|name| (*name).into())
            .collect();
        if !picker.names.contains(&self.config.theme) {
            picker.names.push(self.config.theme.clone());
        }
        picker.names.sort();
        picker.filter("");
        picker.selected = picker
            .filtered
            .iter()
            .position(|name| name == &self.config.theme)
            .unwrap_or(0);
        picker.highlight = true;
        self.menu.themes = Some(picker);
        self.show_dialog(Page::Themes, window, cx, |this, dialog, weak, _, cx| {
            this.theme_picker_dialog(dialog, weak, cx)
        });
        command.update(cx, |command, cx| command.focus(window, cx));
        self.discover_picker_themes(cx);
        cx.notify();
    }

    fn theme_picker_dialog(
        &self,
        dialog: Dialog,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Dialog {
        let Some(picker) = &self.menu.themes else {
            return dialog;
        };
        let (query, select, confirm) = (weak.clone(), weak.clone(), weak.clone());
        let current = self.config.theme.clone();
        let items = picker.filtered.iter().map(|name| {
            CommandItem::new()
                .label(name.clone())
                .checked(*name == current)
        });
        dialog.title("Color Scheme").w(px(520.)).child(
            v_flex()
                .gap_2()
                .child(
                    Command::new(&picker.command)
                        .bordered(false)
                        .filterable(false)
                        .max_h(px(380.))
                        .placeholder("Search themes...")
                        .items(items)
                        .empty(|_, _, _| "No matching themes. Try a shorter search.")
                        .on_query(move |text, _, cx| {
                            let _ = query.update(cx, |this, cx| {
                                if let Some(picker) = &mut this.menu.themes {
                                    if picker.accepting || picker.query == text {
                                        return;
                                    }
                                    picker.filter(text);
                                }
                                this.preview_picker_selection(cx);
                            });
                        })
                        .on_select(move |index, _, cx| {
                            let _ = select.update(cx, |this, cx| {
                                if let Some(picker) = &mut this.menu.themes {
                                    if picker.accepting || picker.highlight {
                                        return;
                                    }
                                    picker.selected = index.row;
                                }
                                this.preview_picker_selection(cx);
                            });
                        })
                        .on_confirm(move |index, _, cx| {
                            let _ = confirm.update(cx, |this, cx| {
                                let name = this
                                    .menu
                                    .themes
                                    .as_ref()
                                    .and_then(|picker| picker.filtered.get(index.row).cloned());
                                if let Some(name) = name {
                                    this.apply_picker_theme(&name, cx);
                                }
                            });
                        }),
                )
                .child(
                    div()
                        .debug_selector(|| "theme-status".into())
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "{} of {} themes. {}",
                            picker.filtered.len(),
                            picker.names.len(),
                            picker.status()
                        )),
                ),
        )
    }

    /// Moves the kit highlight to the theme the picker opened on, once the
    /// list has been rendered with rows. Polled, since the rows only exist
    /// after the dialog's first frame.
    pub(crate) fn sync_theme_highlight(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.page != Some(Page::Themes) {
            return;
        }
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if !picker.highlight || picker.command.read(cx).matched_count() == 0 {
            return;
        }
        picker.highlight = false;
        let row = picker.selected;
        picker.command.update(cx, |command, cx| {
            command.set_selected_index(Some(IndexPath::new(row)), window, cx)
        });
    }

    fn discover_picker_themes(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.discovering {
            return;
        }
        picker.discovering = true;
        let session = picker.session;
        let config = self.config.clone();
        let discovery = cx
            .background_executor()
            .spawn(async move { config.available_themes() });
        cx.spawn(async move |this, cx| {
            let result = discovery.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_picker_discovery(session, result, cx);
            });
        })
        .detach();
    }

    fn finish_picker_discovery(
        &mut self,
        session: u64,
        result: crate::Result<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        picker.discovering = false;
        if picker.baseline.is_none() {
            return;
        }
        match result {
            Ok(mut names) => {
                // Theme directories are process-wide, not picker-session state.
                // Reuse a running scan on reopen, adding the current explicit selection.
                if !names.contains(&self.config.theme) {
                    names.push(self.config.theme.clone());
                    names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
                }
                let selected = picker.filtered.get(picker.selected).cloned();
                picker.names = names;
                let query = picker.query.clone();
                picker.filter(&query);
                if let Some(index) = picker
                    .filtered
                    .iter()
                    .position(|name| Some(name) == selected.as_ref())
                {
                    picker.selected = index;
                }
            }
            Err(error) if picker.session == session => picker.error = Some(error.to_string()),
            Err(_) => {}
        }
        if !picker.query.is_empty() && picker.desired.is_none() {
            self.preview_picker_selection(cx);
        }
        cx.notify();
    }

    pub(crate) fn theme_save_in_flight(&self) -> bool {
        self.menu
            .themes
            .as_ref()
            .is_some_and(|picker| picker.saving)
    }

    pub(crate) fn cancel_theme_preview(&mut self, cx: &mut Context<Self>) -> bool {
        // Starting the disk write is the commit boundary. Its result must be
        // reconciled before another modal, cancellation, or reload can proceed.
        if self.theme_save_in_flight() {
            return false;
        }
        if let Some(picker) = &mut self.menu.themes {
            if let Some(theme) = picker.baseline.take() {
                self.theme = theme;
                cx.notify();
            }
            picker.session += 1;
            picker.desired = None;
            picker.accepting = false;
        }
        true
    }

    fn preview_picker_selection(&mut self, cx: &mut Context<Self>) {
        if self.menu.page != Some(Page::Themes) {
            return;
        }
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.accepting {
            return;
        }
        let desired = picker.filtered.get(picker.selected).cloned();
        if picker.desired != desired {
            picker.request += 1;
            picker.desired = desired;
            picker.loaded = None;
            picker.error = None;
            if let Some(theme) = picker
                .desired
                .as_deref()
                .and_then(|name| Theme::builtin(name.trim()))
            {
                self.theme = theme;
                picker.loaded = picker.desired.clone();
            } else if picker.desired.is_none()
                && let Some(theme) = &picker.baseline
            {
                self.theme = theme.clone();
            }
        }
        self.drive_picker_load(cx);
        cx.notify();
    }

    fn apply_picker_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.accepting {
            return;
        }
        if let Some(index) = picker
            .filtered
            .iter()
            .position(|candidate| candidate == name)
        {
            picker.selected = index;
        } else {
            return;
        }
        self.preview_picker_selection(cx);
        if let Some(picker) = &mut self.menu.themes {
            picker.accepting = true;
        }
        self.drive_picker_load(cx);
    }

    fn drive_picker_load(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.baseline.is_none() || picker.in_flight.is_some() {
            return;
        }
        let Some(name) = picker.desired.clone() else {
            return;
        };
        if picker.loaded.as_ref() == Some(&name) && !picker.accepting {
            return;
        }
        let token = (picker.session, picker.request);
        let saving = picker.accepting && picker.loaded.as_ref() == Some(&name);
        picker.saving = saving;
        picker.in_flight = Some(token);
        let window = picker.window;
        let mut config = self.config.clone();
        config.theme = name.clone();
        let theme = self.theme.clone();
        let task = cx.background_executor().spawn(async move {
            if saving {
                config.save_theme(&name).map(|()| theme)
            } else {
                config.theme()
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = window.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.finish_picker_load(token, saving, result, window, cx)
                });
            });
        })
        .detach();
    }

    fn finish_picker_load(
        &mut self,
        token: (u64, u64),
        saving: bool,
        result: crate::Result<Theme>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.in_flight != Some(token) {
            return;
        }
        picker.in_flight = None;
        picker.saving = false;
        if picker.baseline.is_none() || token != (picker.session, picker.request) {
            self.drive_picker_load(cx);
            return;
        }
        match result {
            Ok(theme) => {
                self.theme = theme;
                picker.loaded = picker.desired.clone();
                if saving {
                    if let Some(name) = &picker.desired {
                        self.config.theme = name.clone();
                    }
                    picker.baseline = None;
                    self.dismiss_menu(window, cx);
                } else {
                    self.drive_picker_load(cx);
                }
            }
            Err(error) => {
                picker.error = Some(error.to_string());
                picker.accepting = false;
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core::prelude::v1::test;

    #[gpui_kit::test]
    fn saving_blocks_cancel_replacement_and_reload_until_reconciled(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(800.), px(600.)));
        for success in [true, false] {
            let token = cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.config.theme = "Default".into();
                    view.theme = Theme::default();
                    view.open_theme_picker(window, cx);
                    let picker = view.menu.themes.as_mut().unwrap();
                    picker.filtered = vec!["Nord".into()];
                    picker.selected = 0;
                    view.preview_picker_selection(cx);
                    let picker = view.menu.themes.as_mut().unwrap();
                    let token = (picker.session, picker.request);
                    // Hold completion at the disk-write boundary without touching personal config.
                    picker.accepting = true;
                    picker.saving = true;
                    picker.in_flight = Some(token);
                    token
                })
            });
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_menu(window, cx);
                    assert!(!view.open_menu(window, cx));
                    view.open_preferences(window, cx);
                    view.open_keybinds(window, cx);
                    view.open_theme_picker(window, cx);
                    view.open_palette(false, window, cx);
                    view.show_install_modal(window, cx);
                    view.open_app_update(false, window, cx);
                    view.open_app_update(true, window, cx);
                    assert!(view.update_preview.is_none());
                    let snapshot: herdr_client::protocol::ClientShellSnapshot =
                        serde_json::from_str(include_str!(
                            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
                        ))
                        .unwrap();
                    let tab = snapshot.tabs[0].tab_id.clone();
                    view.live.snapshot = Some(std::sync::Arc::new(snapshot));
                    view.open_tab_menu(&tab, Point::default(), window, cx);
                    view.open_tab_close(&tab, window, cx);
                    view.open_close_confirmation(crate::controls::Command::CloseTab, window, cx);
                    view.reload_gui_config(window, cx);
                    assert!(view.menu.page == Some(Page::Themes));
                    assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                    assert_eq!(view.config.theme, "Default");
                    let picker = view.menu.themes.as_ref().unwrap();
                    assert_eq!((picker.session, picker.request), token);
                    assert!(picker.saving);
                    let result = if success {
                        Ok(Theme::builtin("Nord").unwrap())
                    } else {
                        Err(crate::Error::MissingHome)
                    };
                    view.finish_picker_load(token, true, result, window, cx);
                    assert!(!view.theme_save_in_flight());
                    if success {
                        assert_eq!(view.config.theme, "Nord");
                        assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                        assert!(view.menu.page.is_none());
                    } else {
                        assert!(view.menu.page == Some(Page::Themes));
                        assert!(view.menu.themes.as_ref().unwrap().error.is_some());
                        view.dismiss_menu(window, cx);
                        assert_eq!(view.theme, Theme::default());
                        assert_eq!(view.config.theme, "Default");
                    }
                })
            });
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_theme_picker(window, cx);
                let picker = view.menu.themes.as_mut().unwrap();
                let token = (picker.session, picker.request);
                picker.in_flight = Some(token);
                picker.filtered = vec!["/pending-theme".into()];
                picker.selected = 0;
                view.apply_picker_theme("/pending-theme", cx);
                assert!(!view.theme_save_in_flight());
                view.dismiss_menu(window, cx);
                view.finish_picker_load(
                    token,
                    false,
                    Ok(Theme::builtin("Nord").unwrap()),
                    window,
                    cx,
                );
                assert!(view.menu.page.is_none());
                assert_eq!(view.theme, Theme::default());
                assert_eq!(view.config.theme, "Default");
                assert!(view.menu.themes.as_ref().unwrap().in_flight.is_none());
            })
        });
    }

    #[gpui_kit::test]
    fn discovery_is_single_flight_across_reopens_and_ignores_old_errors(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_theme_picker(window, cx)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let picker = view.menu.themes.as_mut().unwrap();
                let session = picker.session;
                picker.discovering = true;
                for _ in 0..30 {
                    view.dismiss_menu(window, cx);
                    view.open_theme_picker(window, cx);
                    assert!(view.menu.themes.as_ref().unwrap().discovering);
                }
                view.finish_picker_discovery(
                    session,
                    Ok(vec!["Default".into(), "scan-result".into()]),
                    cx,
                );
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let picker = view.menu.themes.as_mut().unwrap();
                assert!(!picker.discovering);
                assert_eq!(picker.names, ["Default", "scan-result"]);
                let session = picker.session;
                picker.discovering = true;
                view.dismiss_menu(window, cx);
                view.open_theme_picker(window, cx);
                view.finish_picker_discovery(session, Err(crate::Error::MissingHome), cx);
                let picker = view.menu.themes.as_ref().unwrap();
                assert!(!picker.discovering);
                assert!(picker.error.is_none());
            })
        });
    }

    #[gpui_kit::test]
    fn load_completion_coalesces_and_fences_requests_and_sessions(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_theme_picker(window, cx);
                // Hold the single load slot explicitly. No filesystem or timing guesses.
                let picker = view.menu.themes.as_mut().unwrap();
                let old = (picker.session, picker.request);
                picker.in_flight = Some(old);
                picker.filtered = vec!["file-a".into(), "file-b".into(), "Nord".into()];
                picker.selected = 0;
                view.preview_picker_selection(cx);
                view.menu.themes.as_mut().unwrap().selected = 1;
                view.preview_picker_selection(cx);
                assert_eq!(view.menu.themes.as_ref().unwrap().in_flight, Some(old));
                assert_eq!(
                    view.menu.themes.as_ref().unwrap().desired.as_deref(),
                    Some("file-b")
                );
                view.menu.themes.as_mut().unwrap().selected = 2;
                view.preview_picker_selection(cx);
                view.finish_picker_load(old, false, Err(crate::Error::MissingHome), window, cx);
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                assert!(view.menu.themes.as_ref().unwrap().error.is_none());
                assert!(view.menu.themes.as_ref().unwrap().in_flight.is_none());

                let picker = view.menu.themes.as_mut().unwrap();
                let old = (picker.session, picker.request);
                picker.in_flight = Some(old);
                view.dismiss_menu(window, cx);
                view.open_theme_picker(window, cx);
                view.finish_picker_load(
                    old,
                    false,
                    Ok(Theme::builtin("Dracula").unwrap()),
                    window,
                    cx,
                );
                assert_eq!(view.theme, Theme::default());
                assert!(view.menu.themes.as_ref().unwrap().error.is_none());
                assert!(view.menu.themes.as_ref().unwrap().in_flight.is_none());
                // A duplicate/foreign completion cannot release another request's slot.
                view.menu.themes.as_mut().unwrap().in_flight = Some((999, 999));
                view.finish_picker_load(old, false, Err(crate::Error::MissingHome), window, cx);
                assert_eq!(
                    view.menu.themes.as_ref().unwrap().in_flight,
                    Some((999, 999))
                );
            })
        });
    }

    #[gpui_kit::test]
    fn background_load_runs_latest_target_and_failure_preserves_preview(cx: &mut TestAppContext) {
        let path =
            std::env::temp_dir().join(format!("herdr-picker-preview-{}", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        use std::io::Write as _;
        file.write_all(b"background=123456").unwrap();
        drop(file);
        let name = path.to_str().unwrap().to_owned();
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_theme_picker(window, cx);
                let picker = view.menu.themes.as_mut().unwrap();
                let old = (picker.session, picker.request);
                picker.in_flight = Some(old);
                picker.filtered = vec!["/obsolete-theme".into(), name.clone()];
                picker.selected = 0;
                view.preview_picker_selection(cx);
                view.menu.themes.as_mut().unwrap().selected = 1;
                view.preview_picker_selection(cx);
                // Completing an obsolete request starts exactly the latest file load.
                view.finish_picker_load(old, false, Err(crate::Error::MissingHome), window, cx);
                let picker = view.menu.themes.as_ref().unwrap();
                assert_eq!(picker.in_flight, Some((picker.session, picker.request)));
                assert_eq!(view.theme, Theme::default());
            })
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.theme.background, 0x123456);
            assert_eq!(view.config.theme, "Default");
            assert!(view.menu.themes.as_ref().unwrap().in_flight.is_none());
        });
        std::fs::remove_file(&path).unwrap();
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let picker = view.menu.themes.as_mut().unwrap();
                picker.filtered = vec![name];
                picker.selected = 0;
                picker.desired = None;
                view.preview_picker_selection(cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.theme.background, 0x123456);
                assert!(view.menu.themes.as_ref().unwrap().error.is_some());
                assert_eq!(view.config.theme, "Default");
                view.dismiss_menu(window, cx);
                assert_eq!(view.theme, Theme::default());
            })
        });
    }

    #[gpui_kit::test]
    fn accept_completion_retains_preview_and_failure_remains_cancellable(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_theme_picker(window, cx);
                let picker = view.menu.themes.as_mut().unwrap();
                picker.filtered = vec!["Nord".into()];
                picker.selected = 0;
                view.preview_picker_selection(cx);
                let picker = view.menu.themes.as_mut().unwrap();
                let token = (picker.session, picker.request);
                picker.in_flight = Some(token);
                view.apply_picker_theme("Nord", cx);
                assert!(view.menu.themes.as_ref().unwrap().accepting);
                assert_eq!(view.config.theme, "Default");
                view.finish_picker_load(token, true, Err(crate::Error::MissingHome), window, cx);
                assert!(view.menu.themes.as_ref().unwrap().error.is_some());
                assert_eq!(view.config.theme, "Default");
                view.dismiss_menu(window, cx);
                assert_eq!(view.theme, Theme::default());

                view.open_theme_picker(window, cx);
                let picker = view.menu.themes.as_mut().unwrap();
                picker.filtered = vec!["Nord".into()];
                picker.selected = 0;
                view.preview_picker_selection(cx);
                let picker = view.menu.themes.as_mut().unwrap();
                let token = (picker.session, picker.request);
                picker.in_flight = Some(token);
                view.apply_picker_theme("Nord", cx);
                view.finish_picker_load(
                    token,
                    true,
                    Ok(Theme::builtin("Nord").unwrap()),
                    window,
                    cx,
                );
                assert!(view.menu.page.is_none());
                assert_eq!(view.config.theme, "Nord");
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
                view.dismiss_menu(window, cx);
                assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
            })
        });
    }
}

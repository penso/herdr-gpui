//! Command dispatch and focus changes. A focus change is fenced behind an
//! ordered surface barrier so input cannot reach the previous pane while the
//! navigation and its projection are still in flight.

use super::HerdrWindow;
use crate::{
    config::{FONT_SIZE_RANGE, FONT_SIZE_STEP},
    controls::{self, Command},
    log_window,
    navigation::{NavigationTarget, OwnedNavigationTarget},
    open_additional_window, state,
};
use gpui::{Context, Window};
use std::time::Duration;

impl HerdrWindow {
    pub(crate) fn navigate(
        &mut self,
        target: NavigationTarget<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() {
            return false;
        }
        self.dispatch_navigation(target, cx)
    }

    /// Complete an accepted navigation even if its context menu has since opened.
    pub(crate) fn dispatch_navigation(
        &mut self,
        target: NavigationTarget<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.input_ready() {
            return false;
        }
        let queued =
            self.request_focus_change("Navigate", Some((&target).into()), |handle, boot| {
                match target {
                    NavigationTarget::Workspace(id) => handle.focus_workspace(boot, id),
                    NavigationTarget::Tab(id) => handle.focus_tab(boot, id),
                    NavigationTarget::Pane(id) => handle.focus_pane(boot, id),
                }
            });
        self.marked.clear();
        cx.notify();
        queued
    }

    /// `label` is what a failure is reported as, not a method name: navigation
    /// picks its method from the target, so it reports itself by name.
    pub(crate) fn request_focus_change(
        &mut self,
        label: &str,
        focus: Option<OwnedNavigationTarget>,
        enqueue: impl FnOnce(
            &herdr_client::ClientHandle,
            &str,
        ) -> Result<String, herdr_client::SendError>,
    ) -> bool {
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) {
            if let Err(error) = enqueue(handle, &snapshot.boot_id) {
                self.local_error = Some(format!("{label}: {error}"));
            } else {
                self.fence_focus_change(focus);
                return true;
            }
        }
        false
    }

    pub(crate) fn fence_focus_change(&mut self, focus: Option<OwnedNavigationTarget>) {
        if !self.live.supports_surface {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) && let Ok(mut state) = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .lock()
        {
            // An ordered surface barrier prevents input hitting the previous
            // pane while navigation/creation and its projection are in flight.
            // Register the barrier under the inbox lock before input can resume.
            let (request, failed) = match handle.set_surface_active(&snapshot.boot_id, true) {
                Ok(request) => (request, false),
                Err(error) => {
                    self.local_error = Some(error.to_string());
                    (String::new(), true)
                }
            };
            state.activation = Some(state::SurfaceActivation {
                request,
                boot: snapshot.boot_id.clone(),
                revision: None,
                failed,
                focus,
                active: true,
            });
            state.surface = None;
            state.dirty = true;
            self.live = state.clone();
            self.activation_deadline = Some(
                std::time::Instant::now()
                    + if failed {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(5)
                    },
            );
        }
    }

    /// Applies a session terminal size. Painting, hit testing, and IME
    /// placement all read `config.terminal.size` and its derived line height,
    /// so writing that one field keeps the three in agreement; render
    /// re-measures the cell and the canvas resends the geometry.
    ///
    /// A pending config load is deliberately left alone. Cancelling it the way
    /// the theme picker does would strand the very first load, which has no
    /// retry, on default fonts; a landing reload merely discards the
    /// adjustment, which is what reloading is for.
    pub(crate) fn set_terminal_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        let size = size.clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end());
        if size == self.config.terminal.size {
            return;
        }
        self.config.terminal.size = size;
        // The console follows the rendered terminal face, as a reload makes it.
        log_window::set_appearance(&self.config, &self.theme, cx);
        cx.notify();
    }

    pub(crate) fn command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            return;
        }
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.actions += 1;
        }
        match command {
            Command::OpenNotificationTarget => {
                if let Some((endpoint, id)) = self.endpoints.iter().find_map(|e| {
                    e.toasts
                        .entries
                        .iter()
                        .find(|(_, n)| n.visible)
                        .map(|(id, _)| (e, *id))
                }) {
                    let (origin, generation, inbox) = (
                        endpoint.id.clone(),
                        endpoint.generation,
                        endpoint.connection.inbox.clone(),
                    );
                    self.click_toast(&origin, generation, &inbox, id, cx);
                }
                return;
            }
            Command::Logs => {
                log_window::open(cx);
                return;
            }
            Command::NewWindow => {
                // Another client of the same launch target, not another daemon.
                open_additional_window(self.endpoints[0].connection.target.clone(), cx);
                return;
            }
            Command::ClosePane | Command::CloseTab => {
                self.open_close_confirmation(command, window, cx);
                return;
            }
            Command::Palette | Command::WorkspacePicker => {
                self.open_palette(command == Command::WorkspacePicker, window, cx);
                return;
            }
            Command::NewWorktree => {
                self.open_new_worktree(window, cx);
                return;
            }
            Command::Keybinds => {
                self.open_keybinds(window, cx);
                return;
            }
            Command::Themes => {
                self.open_theme_picker(window, cx);
                return;
            }
            Command::Settings => {
                self.open_preferences(window, cx);
                return;
            }
            Command::About => {
                self.open_about(window, cx);
                return;
            }
            Command::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            // A hidden sidebar is shown in the mode it had, rather than
            // switched to a mode nobody can see.
            Command::CollapseSidebar => {
                if self.sidebar_visible {
                    self.sidebar_mode = self.sidebar_mode.toggled();
                } else {
                    self.sidebar_visible = true;
                }
                self.sidebar_mode_modified = true;
                self.save_chrome();
                cx.notify();
            }
            Command::IncreaseFontSize | Command::DecreaseFontSize => {
                let step = if command == Command::IncreaseFontSize {
                    FONT_SIZE_STEP
                } else {
                    -FONT_SIZE_STEP
                };
                self.set_terminal_font_size(self.config.terminal.size + step, cx);
            }
            Command::ResetFontSize => {
                self.set_terminal_font_size(self.configured_terminal_size, cx);
            }
            Command::ClearPane if !self.live.supports_pane_clear => {
                // Older daemons do not advertise `pane.clear`. Say so rather than
                // typing `clear` into the pane, which could reach a running program.
                self.local_error = Some("Clear Pane needs a newer Herdr daemon.".into());
                cx.notify();
                return;
            }
            Command::Reconnect => self.reconnect(),
            Command::Quit => {
                cx.quit();
                return;
            }
            _ => {}
        }
        if self.activation_deadline.is_some()
            || !self.endpoints[self.selected_endpoint].surface_requested()
        {
            return;
        }
        if let Some(snapshot) = &self.live.snapshot
            && let Some((method, params)) = controls::request(command, snapshot)
        {
            self.request_focus_change(method.as_str(), None, |handle, boot| {
                handle.request(boot, method, params)
            });
            self.marked.clear();
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }
}

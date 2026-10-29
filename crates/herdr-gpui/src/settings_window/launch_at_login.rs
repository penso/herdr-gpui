//! General > Startup's **Launch at login** row, on Windows. The registry is
//! the setting (`crate::launch_at_login`), read when the window opens, each
//! time it comes to the front (Task Manager may have changed it meanwhile),
//! and after each change, so the row shows what Windows will do.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::i18n::fl;
use crate::launch_at_login::{self, State};
use gpui::{prelude::*, *};

/// What the row knows of the startup entry.
#[derive(Default)]
pub(super) struct LaunchAtLogin {
    /// As last read; `None` before the first read, or after a failed one.
    pub(super) state: Option<State>,
    /// A read or write is running; another waits for the next activation.
    pub(super) busy: bool,
    _activation: Option<Subscription>,
}

impl SettingsWindow {
    /// Reads the entry again whenever `window` becomes active.
    pub(super) fn watch_launch_at_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.launch_at_login._activation =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refresh_launch_at_login(None, cx);
                }
            }));
    }

    /// Reads the startup entry off the UI thread, first writing it when
    /// `change` is `Some(on)`. One runs at a time, and the toggle is
    /// disabled meanwhile, so a read never overlaps a write.
    pub(super) fn refresh_launch_at_login(&mut self, change: Option<bool>, cx: &mut Context<Self>) {
        if self.launch_at_login.busy {
            return;
        }
        self.launch_at_login.busy = true;
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            let saved = change.map_or(Ok(()), launch_at_login::set);
            (saved, launch_at_login::state())
        });
        cx.spawn(async move |this, cx| {
            let (saved, state) = work.await;
            let _ = this.update(cx, |this, cx| {
                this.launch_at_login.busy = false;
                if let Err(error) = &saved {
                    tracing::warn!(%error, "Could not change launch at login");
                    this.error = Some(fl!(
                        "settings-general-error-launch-at-login",
                        error = error.to_string()
                    ));
                }
                match state {
                    Ok(state) => this.launch_at_login.state = Some(state),
                    Err(error) => {
                        this.launch_at_login.state = None;
                        tracing::warn!(%error, "Could not read launch at login");
                        if saved.is_ok() {
                            this.error = Some(fl!(
                                "settings-general-error-launch-at-login",
                                error = error.to_string()
                            ));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn launch_at_login_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = &self.theme;
        let state = self.launch_at_login.state;
        let on = state == Some(State::On);
        let ready = !self.launch_at_login.busy && matches!(state, Some(State::On | State::Off));
        let row = ui::row(
            theme,
            "general-launch-at-login",
            fl!("settings-general-launch-at-login"),
        )
        .description(fl!("settings-general-launch-at-login-description"));
        // Windows' own switch wins; the app never clears its mark.
        let row = if state == Some(State::DisabledByWindows) {
            row.disabled(fl!("settings-general-launch-at-login-disabled"))
        } else {
            row
        };
        let toggle =
            ui::toggle(theme, "general-launch-at-login-toggle", on, ready).when(ready, |toggle| {
                toggle.on_click(
                    cx.listener(move |this, _, _, cx| this.refresh_launch_at_login(Some(!on), cx)),
                )
            });
        vec![
            ui::section(theme, &fl!("settings-general-section-startup"), None).into_any_element(),
            row.switch(toggle).into_any_element(),
        ]
    }
}

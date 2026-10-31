//! The completed Add Device state: the device is saved before this is shown.
//! Recovery advice wraps and stays available until the user closes the dialog.
use super::{Page, add_device::Step, setup::SaveOutcome};
use crate::HerdrWindow;
use crate::i18n::fl;
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(super) fn device_save_succeeded(&mut self, outcome: SaveOutcome, cx: &mut Context<Self>) {
        // Saving is already complete. Keep recovery advice in the scrollable
        // dialog until dismissed, rather than timing out a truncated flash.
        if let Some(form) = &mut self.menu.device_setup {
            form.step = outcome.warning().map_or(Step::Saved, Step::SavedWarning);
            form.error = None;
        }
        cx.notify();
    }

    /// Handle completed saves before polling any setup workspace in progress.
    pub(super) fn poll_saved_device(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.menu.device_setup.as_ref().map(|form| &form.step) {
            Some(Step::Saved) => {
                // While another source of Add Device shows, only the finished
                // SSH form goes; the next visit to SSH starts a fresh one.
                if self.menu.page != Some(Page::AddDevice) {
                    self.menu.device_setup = None;
                    cx.notify();
                } else {
                    self.dismiss_menu(window, cx);
                }
                true
            }
            Some(Step::SavedWarning(_)) => {
                // The summary replaced the input fields. Focus its menu so
                // Enter/Escape work, without taking focus from another source.
                if self.menu.page == Some(Page::AddDevice) && !self.menu.focus.is_focused(window) {
                    window.focus(&self.menu.focus, cx);
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn render_device_saved_warning(
        &self,
        warning: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("device-saved-warning-scroll")
                    .debug_selector(|| "device-saved-warning-scroll".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(16.))
                    .child(
                        div()
                            .debug_selector(|| "device-saved-warning".into())
                            .w_full()
                            .whitespace_normal()
                            .text_color(rgb(self.theme.ink(self.theme.palette[3])))
                            .child(crate::i18n::message(warning, &[])),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .p(px(16.))
                    .border_t_1()
                    .border_color(rgb(self.theme.active))
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .id("device-saved-done")
                            .debug_selector(|| "device-saved-done".into())
                            .p(px(8.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(self.theme.active))
                            .cursor_pointer()
                            .hover(|s| {
                                s.bg(rgb(self.theme.active)
                                    .blend(rgba((self.theme.foreground << 8) | 0x20)))
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_menu(window, cx);
                            }))
                            .child(fl!("devices-setup-done")),
                    ),
            )
    }
}

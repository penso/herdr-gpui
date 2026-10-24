//! The status bar's sign that a phone is connected: a phone and the count,
//! shown only while one is. Clicking it opens Settings > Remote control.

use super::supervisor::Supervisor;
use crate::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn remote_indicator(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let connected = Supervisor::connected(cx);
        if connected == 0 || !self.config.remote.enabled {
            return None;
        }
        let label: SharedString = match connected {
            1 => "1 phone connected for remote control".into(),
            count => format!("{count} phones connected for remote control").into(),
        };
        let (foreground, surface) = (self.theme.foreground, self.theme.surface);
        let hint = label.clone();
        Some(
            div()
                .id("status-remote")
                .debug_selector(|| "status-remote".into())
                .map(|control| {
                    self.chrome_stop(crate::window::chrome_focus::Region::StatusBar, control)
                })
                .role(Role::Button)
                .aria_label(label)
                .flex()
                .flex_none()
                .items_center()
                .gap(px(4.))
                .px_2()
                .h_full()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(self.theme.active)))
                .child(
                    svg()
                        .path("icons/phone.svg")
                        .size(px(14.))
                        .flex_none()
                        .text_color(rgb(crate::menu::online(&self.theme))),
                )
                .when(connected > 1, |button| button.child(connected.to_string()))
                .tooltip(move |_, cx| {
                    cx.new(|_| crate::usage::Hint {
                        text: hint.clone(),
                        foreground,
                        surface,
                    })
                    .into()
                })
                .on_click(cx.listener(|_, _, _, cx| {
                    crate::settings_window::open_remote(cx.entity().downgrade(), cx);
                })),
        )
    }
}

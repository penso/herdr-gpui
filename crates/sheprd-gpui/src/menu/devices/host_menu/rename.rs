//! The page that names a device.
use super::HostMenu;
use crate::{SheprdWindow, i18n::fl};
use gpui::{prelude::*, *};

impl SheprdWindow {
    /// A centered modal like Add Device: header with the title and Close,
    /// the name field in the body, and the action in the footer.
    pub(super) fn render_rename_device(&self, host: &HostMenu, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let header = div()
            .debug_selector(|| "rename-device-header".into())
            .flex_none()
            .p(px(16.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.config.ui.size * 1.35))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("host-menu-rename-title")),
            )
            .child(
                div()
                    .id("rename-device-close")
                    .debug_selector(|| "rename-device-close".into())
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child(fl!("host-menu-close"))
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx))),
            );
        let body = div()
            .debug_selector(|| "rename-device".into())
            .min_h_0()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(format!("{} · {}", host.target, host.session)), // i18n: data
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(fl!("host-menu-rename-name"))
                    .when_some(host.input.clone(), |field, input| {
                        if host.renaming {
                            field.child(div().child(input.read(cx).text().to_owned()))
                        } else {
                            field.child(input)
                        }
                    }),
            )
            .when_some(host.error.clone(), |body, error| {
                body.child(div().text_color(crate::menu::danger(theme)).child(error))
            });
        let ready = !host.renaming;
        let footer = div()
            .debug_selector(|| "rename-device-footer".into())
            .flex_none()
            .p(px(16.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .justify_end()
            .child(
                div()
                    .id("rename-device-submit")
                    .debug_selector(|| "rename-device-submit".into())
                    .p(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.active))
                    .when(ready, |button| {
                        button.cursor_pointer().hover(|s| {
                            s.bg(rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x20)))
                        })
                    })
                    .when(!ready, |button| button.text_color(rgb(theme.muted)))
                    .child(if ready {
                        fl!("host-menu-rename-submit")
                    } else {
                        fl!("host-menu-renaming")
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.submit_rename_device(window, cx)),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .child(header)
            .child(body)
            .child(footer)
    }
}

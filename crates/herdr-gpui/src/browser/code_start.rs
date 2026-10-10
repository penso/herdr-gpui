//! The VS Code tab's request for the license the app accepts for VS Code's
//! server when it starts one. Until the user accepts it, here or in
//! Settings > Code, the app starts nothing.
use super::code_view::centered;
#[cfg(not(test))]
use crate::config::{CodeEdit, Config};
use crate::{
    HerdrWindow,
    code_server::launcher::{LICENSE_TERMS, LICENSE_URL, PRIVACY_URL},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The terms, links to read them, and the button that accepts them.
    pub(super) fn render_code_consent(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let link = |id: &'static str, label: &'static str, url: &'static str| {
            div()
                .id(id)
                .text_color(crate::menu::accent(theme))
                .cursor_pointer()
                .hover(|style| style.underline())
                .on_click(move |_, _, cx| cx.open_url(url))
                .child(label)
        };
        let accept = div()
            .id("code-accept")
            .debug_selector(|| "code-accept".into())
            .px(px(14.))
            .py(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(crate::menu::accent(theme))
            .bg(rgb(theme.active))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.surface)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.accept_code_license(cx);
            }))
            .child("Accept and start");
        centered(
            div()
                .debug_selector(|| "code-consent".into())
                .flex()
                .flex_col()
                .items_center()
                .gap(px(12.))
                .max_w(px(420.))
                .text_center()
                .child(
                    div()
                        .text_color(rgb(theme.foreground))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Start VS Code"),
                )
                .child(div().text_color(rgb(theme.muted)).child(LICENSE_TERMS))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap(px(16.))
                        .child(link("code-license-link", "License Terms", LICENSE_URL))
                        .child(link("code-privacy-link", "Privacy Statement", PRIVACY_URL)),
                )
                .child(accept),
        )
    }

    /// Remembers that the user accepted the license, and starts VS Code at
    /// once rather than after the config reloads.
    pub(crate) fn accept_code_license(&mut self, cx: &mut Context<Self>) {
        self.config.code.license_accepted = true;
        // Tests never write the user's config.
        #[cfg(not(test))]
        self.write_preference(|| Config::save_code(CodeEdit::AcceptLicense), cx);
        cx.notify();
    }
}

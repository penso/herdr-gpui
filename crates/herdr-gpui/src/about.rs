//! Classic macOS-style About box: build identity only. It reads embedded build
//! constants, never the daemon or the network; links open in the browser.
// i18n: migrated
use crate::i18n::fl;
use crate::{HerdrWindow, menu::Page};
use gpui::{prelude::*, *};
use std::sync::{Arc, LazyLock};

pub(super) const WEBSITE: &str = "https://herdr.dev/";
const HOMEPAGE: &str = "https://herdrui.dev/";
pub(super) const REPOSITORY: &str = "https://github.com/penso/herdr-gpui";
const COPYRIGHT_YEAR: &str = "© 2026";
// i18n: data
const AUTHOR: &str = "Fabien Penso";
const AUTHOR_URL: &str = "https://pen.so";
const TWITTER_URL: &str = "https://x.com/fabienpenso";
// i18n: data
const LICENSE: &str = "· Apache-2.0";

static ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        crate::app_icon::PNG.to_vec(),
    ))
});

impl HerdrWindow {
    pub(super) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::About);
    }

    pub(super) fn render_about(&self, cx: &mut Context<Self>) -> Div {
        let font = &self.config.ui;
        let theme = &self.theme;
        let muted = rgb(theme.muted);
        div()
            .debug_selector(|| "about".into())
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .px(px(24.))
            .py(px(28.))
            .text_center()
            .child(
                div()
                    .debug_selector(|| "about-icon".into())
                    .size(px(96.))
                    .mb(px(6.))
                    .child(img(ICON.clone()).size_full()),
            )
            .child(
                div()
                    .debug_selector(|| "about-name".into())
                    .text_size(px(font.size * 1.8))
                    .line_height(px(font.size * 2.2))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Herdr"), // i18n: data
            )
            .child(
                div()
                    .debug_selector(|| "about-version".into())
                    .text_color(muted)
                    .child(fl!("about-version", version = crate::APP_VERSION)),
            )
            .child(
                div()
                    .debug_selector(|| "about-summary".into())
                    .pt(px(6.))
                    .text_color(muted)
                    .child(fl!("about-summary")),
            )
            .child(
                div()
                    .debug_selector(|| "about-unaffiliated".into())
                    .pt(px(4.))
                    .text_size(px(font.size * 0.85))
                    .text_color(muted)
                    .child(fl!("about-unaffiliated")),
            )
            .children(self.render_about_build(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .pt(px(10.))
                    .child(self.about_link("about-homepage", fl!("about-website"), HOMEPAGE))
                    .child(self.about_link("about-website", "herdr.dev", WEBSITE))
                    // i18n: data
                    .child(self.about_link("about-repository", "GitHub", REPOSITORY)),
            )
            .child(
                div()
                    .debug_selector(|| "about-copyright".into())
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .pt(px(10.))
                    .text_size(px(font.size * 0.85))
                    .text_color(muted)
                    .child(COPYRIGHT_YEAR)
                    .child(self.about_link("about-author", AUTHOR, AUTHOR_URL))
                    .child(
                        self.about_link(
                            "about-twitter",
                            svg()
                                .path("icons/x.svg")
                                .size(px(font.size * 0.85))
                                .text_color(crate::menu::accent(theme)),
                            TWITTER_URL,
                        ),
                    )
                    .child(LICENSE),
            )
            .child(
                div()
                    .id("about-close")
                    .debug_selector(|| "about-close".into())
                    .mt(px(14.))
                    .px(px(16.))
                    .py(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.active))
                    .cursor_pointer()
                    .child(fl!("about-ok"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss_menu(window, cx);
                    })),
            )
    }

    /// Worktree builds are throwaway; name the branch that produced this one.
    fn render_about_build(&self, theme: &crate::config::Theme) -> Option<Div> {
        (env!("HERDR_BUILD_WORKTREE") == "1").then(|| {
            let pr = env!("HERDR_BUILD_PR");
            div()
                .debug_selector(|| "about-build".into())
                .flex()
                .items_center()
                .gap(px(8.))
                .pt(px(6.))
                .max_w_full()
                .text_size(px(self.config.ui.size * 0.85))
                .text_color(rgb(theme.muted))
                .child(
                    div()
                        .debug_selector(|| "about-branch".into())
                        .min_w_0()
                        .truncate()
                        .child(env!("HERDR_BUILD_BRANCH")),
                )
                .when(!pr.is_empty(), |row| {
                    row.child(self.about_link(
                        "about-pr",
                        fl!("about-pull-request", number = pr),
                        crate::worktree_banner::pull_request_url(pr),
                    ))
                })
        })
    }

    fn about_link(
        &self,
        id: &'static str,
        label: impl IntoElement,
        url: impl Into<String>,
    ) -> Stateful<Div> {
        let url = url.into();
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_color(crate::menu::accent(&self.theme))
            .border_b_1()
            .border_color(transparent_black())
            .hover(|style| style.border_color(crate::menu::accent(&self.theme)))
            .child(label)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.open_url(&url);
            })
    }
}

#[cfg(test)]
mod tests;

//! Classic About box: build identity only. It reads embedded build constants,
//! never the daemon or the network; links open in the browser.
use crate::{HerdrWindow, menu::Page};
use gpui_kit::{
    component::{
        ActiveTheme as _,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    prelude::*,
    *,
};
use std::sync::{Arc, LazyLock};

pub(super) const WEBSITE: &str = "https://herdr.dev/";
pub(super) const REPOSITORY: &str = "https://github.com/penso/herdr-gpui";
const COPYRIGHT_YEAR: &str = "© 2026";
const AUTHOR: &str = "Fabien Penso";
const AUTHOR_URL: &str = "https://pen.so";
const TWITTER_URL: &str = "https://x.com/fabienpenso";
const LICENSE: &str = "· Apache-2.0";
const SUMMARY: &str = "Native client for an existing local Herdr daemon.";
const UNAFFILIATED: &str = "An independent project, not affiliated with or endorsed by herdr.dev.";

static ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        crate::app_icon::PNG.to_vec(),
    ))
});

/// A link-styled button that opens `url` in the browser.
fn link(id: &'static str, label: impl Into<SharedString>, url: &'static str) -> Button {
    Button::new(id)
        .link()
        .label(label)
        .on_click(move |_, _, cx| cx.open_url(url))
}

impl HerdrWindow {
    pub(crate) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.begin_menu(window, cx) {
            return;
        }
        self.show_dialog(Page::About, window, cx, |_, dialog, weak, _, cx| {
            let muted = cx.theme().muted_foreground;
            let worktree = env!("HERDR_BUILD_WORKTREE") == "1";
            let pr = env!("HERDR_BUILD_PR");
            dialog
                .w(px(360.))
                .child(
                    v_flex()
                        .debug_selector(|| "about".into())
                        .items_center()
                        .gap_1()
                        .text_center()
                        .child(
                            div()
                                .size(px(96.))
                                .mb_2()
                                .child(img(ICON.clone()).size_full()),
                        )
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Herdr"),
                        )
                        .child(
                            div()
                                .text_color(muted)
                                .child(format!("Version {}", crate::APP_VERSION)),
                        )
                        .child(div().pt_1().text_color(muted).child(SUMMARY))
                        .child(div().text_sm().text_color(muted).child(UNAFFILIATED))
                        // Worktree builds are throwaway; name the branch that produced this one.
                        .when(worktree, |about| {
                            about.child(
                                h_flex()
                                    .gap_2()
                                    .max_w_full()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .child(env!("HERDR_BUILD_BRANCH")),
                                    )
                                    .when(!pr.is_empty(), |row| {
                                        row.child(
                                            Button::new("about-pr")
                                                .link()
                                                .label(format!("PR #{pr}"))
                                                .on_click(move |_, _, cx| {
                                                    cx.open_url(
                                                        &crate::worktree_banner::pull_request_url(
                                                            pr,
                                                        ),
                                                    )
                                                }),
                                        )
                                    }),
                            )
                        })
                        .child(
                            h_flex()
                                .gap_4()
                                .pt_2()
                                .child(link("about-website", "herdr.dev", WEBSITE))
                                .child(link("about-repository", "GitHub", REPOSITORY)),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .pt_2()
                                .text_sm()
                                .text_color(muted)
                                .child(COPYRIGHT_YEAR)
                                .child(link("about-author", AUTHOR, AUTHOR_URL))
                                .child(link("about-twitter", "X", TWITTER_URL))
                                .child(LICENSE),
                        ),
                )
                .footer(h_flex().w_full().justify_center().child(
                    Button::new("about-close").primary().label("OK").on_click(
                        crate::menu::listener(weak, |this, window, cx| {
                            this.dismiss_menu(window, cx)
                        }),
                    ),
                ))
                .on_ok(crate::menu::submit(weak, |this, window, cx| {
                    this.dismiss_menu(window, cx)
                }))
        });
    }
}

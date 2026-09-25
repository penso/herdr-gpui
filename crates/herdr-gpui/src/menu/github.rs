//! GitHub sign-in: the device-code flow and the connected account, in a kit
//! dialog. Nothing here runs a command; links open in the browser.

use super::{Page, listener};
use crate::HerdrWindow;
use gpui_kit::{
    component::{
        ActiveTheme as _, Icon, IconName,
        alert::Alert,
        avatar::Avatar,
        button::{Button, ButtonVariants as _},
        clipboard::Clipboard,
        dialog::Dialog,
        h_flex, v_flex,
    },
    prelude::*,
    *,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Open,
    Start,
    SignOut,
}

impl HerdrWindow {
    /// Opens the GitHub account dialog, starting a sign-in when `connect`
    /// asks for one and none is under way.
    pub(crate) fn open_github(
        &mut self,
        connect: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page != Some(Page::GitHub) {
            if !self.begin_menu(window, cx) {
                return;
            }
            self.show_dialog(Page::GitHub, window, cx, |this, dialog, weak, _, cx| {
                this.github_dialog(dialog, weak, cx)
            });
        }
        if connect && !self.menu.github.connected() && !self.menu.github.loading_profile() {
            self.menu.github.start(&self.config);
        }
        cx.notify();
    }

    pub(crate) fn poll_github(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let connected = self.menu.github.connected();
        if self.menu.github.poll() {
            if !connected && self.menu.github.connected() && self.menu.page == Some(Page::GitHub) {
                self.dismiss_menu(window, cx);
            }
            cx.notify();
        }
    }

    fn github_actions(&self) -> Vec<Action> {
        let auth = &self.menu.github;
        let mut actions = Vec::new();
        if auth.code().is_some() {
            actions.push(Action::Open);
        } else if !auth.busy() && !auth.loading_profile() && !auth.connected() {
            actions.push(Action::Start);
        }
        if auth.can_sign_out() && !auth.busy() && !auth.loading_profile() {
            actions.push(Action::SignOut);
        }
        actions
    }

    fn github_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if !self.github_actions().contains(&action) {
            return;
        }
        match action {
            Action::Open => cx.open_url(crate::github::VERIFY_URL),
            Action::Start => self.menu.github.start(&self.config),
            Action::SignOut => {
                self.menu.pr_cache.clear();
                self.menu.pr.clear();
                self.menu.github.sign_out();
            }
        }
        cx.notify();
    }

    fn github_dialog(&self, dialog: Dialog, weak: &WeakEntity<HerdrWindow>, cx: &App) -> Dialog {
        let auth = &self.menu.github;
        let muted = cx.theme().muted_foreground;
        let mut body = v_flex().debug_selector(|| "github-body".into()).gap_3();
        if let Some(profile) = &auth.profile {
            let avatar = match &profile.avatar {
                Some(image) => Avatar::new().src(image.clone()),
                None => Avatar::new().name(profile.login.clone()),
            };
            body = body.child(
                h_flex().gap_3().child(avatar).child(
                    v_flex()
                        .min_w_0()
                        .child(div().text_color(cx.theme().success).child("Connected"))
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("@{}", profile.login)),
                        ),
                ),
            );
        }
        if let Some(code) = auth.code() {
            let copied = weak.clone();
            body = body
                .child("1. Copy your one-time code")
                .child(
                    h_flex()
                        .gap_3()
                        .p_3()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_2xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(code.to_owned()),
                        )
                        .child(
                            Clipboard::new("github-copy")
                                .value(code.to_owned())
                                .on_copied(move |_, _, cx| {
                                    let _ = copied.update(cx, |this, cx| {
                                        this.menu.github.copy_code();
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .when(auth.copied(), |body| {
                    body.child(div().text_sm().text_color(muted).child("Copied"))
                })
                .child("2. Open GitHub and paste the code")
                .child(div().text_color(muted).child("github.com/login/device"));
        }
        let message = auth.message.clone().unwrap_or_else(|| {
            if auth.loading_profile() {
                "Checking GitHub account..."
            } else if auth.connected() {
                "Your account is ready for pull request lookups."
            } else {
                "Sign in securely in your browser to view pull requests. No GitHub CLI required."
            }
            .into()
        });
        if auth.failed {
            body = body.child(Alert::error("github-status", message));
        } else if !auth.connected() {
            body = body.child(div().text_color(muted).child(message));
        }
        if let Some(note) = auth.store().note(auth.connected()) {
            body = body.child(match note {
                crate::github::Note::Warning(text) => Alert::warning("github-note", text),
                crate::github::Note::Info(text) => Alert::info("github-note", text),
            });
        }
        let buttons = self.github_actions().into_iter().map(|action| {
            let (id, label) = match action {
                Action::Open => ("github-open", "Open GitHub"),
                Action::Start if auth.failed => ("github-start", "Try again"),
                Action::Start => ("github-start", "Sign in"),
                Action::SignOut => ("github-sign-out", "Sign out"),
            };
            let button = Button::new(id)
                .label(label)
                .on_click(listener(weak, move |this, _, cx| {
                    this.github_action(action, cx)
                }));
            match action {
                Action::Open | Action::Start => button.primary(),
                Action::SignOut => button.danger(),
            }
        });
        dialog
            .title(
                h_flex()
                    .gap_2()
                    .child(Icon::new(IconName::Github))
                    .child("GitHub"),
            )
            .w(px(420.))
            .child(body)
            .footer(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .justify_end()
                    .gap_2()
                    .children(buttons)
                    .child(
                        Button::new("github-close")
                            .label("Close")
                            .on_click(listener(weak, |this, window, cx| {
                                this.dismiss_menu(window, cx)
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{Action, Page};
    use crate::github::Auth;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn successful_signin_dismisses_only_the_signin_dialog_and_restores_focus(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.github_fixture(false, window, cx);
                view.menu
                    .github
                    .complete_profile_fixture(Ok(Auth::connected_fixture().profile));
                view.poll_github(window, cx);
                assert!(view.menu.github.connected());
                assert!(view.menu.page.is_none());
                assert!(view.focus.is_focused(window));
                // Renewing an active session must not dismiss a dialog the
                // user deliberately opened.
                view.open_github(false, window, cx);
                view.menu
                    .github
                    .complete_profile_fixture(Ok(Auth::connected_fixture().profile));
                view.poll_github(window, cx);
                assert_eq!(view.menu.page, Some(Page::GitHub));
                view.github_fixture(false, window, cx);
                view.menu
                    .github
                    .complete_profile_fixture(Err(crate::Error::GitHubAuthentication));
                view.poll_github(window, cx);
                assert_eq!(view.menu.page, Some(Page::GitHub));
                assert!(!view.menu.github.connected());
            });
        });
    }

    /// Only the actions the flow allows are offered, so a request in flight
    /// can neither be restarted nor turned into a sign-out.
    #[gpui_kit::test]
    fn actions_follow_the_sign_in_state(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.github_fixture(false, window, cx);
                assert_eq!(view.github_actions(), [Action::Start]);
                view.github_fixture(true, window, cx);
                assert_eq!(view.github_actions(), [Action::Open]);
                view.menu.github = Auth::requesting_fixture();
                assert!(view.github_actions().is_empty());
                view.menu.github = Auth::connected_fixture();
                assert_eq!(view.github_actions(), [Action::SignOut]);
                view.github_action(Action::SignOut, cx);
                assert!(!view.menu.github.connected());
            });
        });
    }
}

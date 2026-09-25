//! Native chrome and GitHub account access.
use crate::{HerdrWindow, pull_request::State as PrState};
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, TITLE_BAR_HEIGHT, TitleBar,
    avatar::Avatar,
    badge::Badge,
    button::{Button, ButtonVariants},
    h_flex,
};
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, RenderOnce, SharedString, Styled, TitlebarOptions, Window, div, point,
    prelude::FluentBuilder, px,
};

/// Native chrome the window draws above its body; popups must clear it.
pub(super) const HEIGHT: f32 = 34.;

impl HerdrWindow {
    fn open_profile(&mut self, connect: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open_github(connect, window, cx);
    }

    /// Git actions for the focused checkout, left of the account slot. Hidden
    /// when no local checkout is tracked, so remote endpoints show no control
    /// that cannot act.
    ///
    /// One set of counts only, so two "+N -M" pairs can never sit side by side
    /// meaning different things. A branch with a prefetched pull request shows
    /// that pull request, and a dot on the Git button when the checkout also
    /// has uncommitted work; the popup says how much. A branch without one
    /// shows what a commit would include right now.
    fn render_git_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.git.tracked()?;
        let colors = cx.theme();
        let (success, danger, warning, muted, magenta) = (
            colors.success,
            colors.danger,
            colors.warning,
            colors.muted_foreground,
            colors.magenta,
        );
        let status = self.git.status();
        let dirty = status.is_some_and(|status| status.dirty());
        let running = self.git.running().is_some();
        let pr = self.git_pull_request();
        let summary = match pr {
            Some(pr) => {
                let url = pr.url.clone();
                let color = match pr.state {
                    PrState::Open if pr.is_draft => muted,
                    PrState::Open => success,
                    PrState::Merged => magenta,
                    PrState::Closed => danger,
                    PrState::Unknown => muted,
                };
                Some(
                    Button::new("titlebar-git-pr-link")
                        .debug_selector(|| "titlebar-git-pr-link".into())
                        .ghost()
                        .xsmall()
                        .tooltip("Open pull request")
                        .child(
                            div()
                                .debug_selector(|| "titlebar-git-pr".into())
                                .text_color(color)
                                .child(format!("#{}", pr.number)),
                        )
                        .child(
                            div()
                                .debug_selector(|| "titlebar-git-pr-additions".into())
                                .text_color(success)
                                .child(format!("+{}", crate::sidebar::compact(pr.additions))),
                        )
                        .child(
                            div()
                                .debug_selector(|| "titlebar-git-pr-deletions".into())
                                .text_color(danger)
                                .child(format!("-{}", crate::sidebar::compact(pr.deletions))),
                        )
                        // The title bar would otherwise start a window move.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            cx.open_url(&url);
                        })
                        .into_any_element(),
                )
            }
            None => status.filter(|status| status.dirty()).map(|status| {
                h_flex()
                    .gap_1()
                    .text_xs()
                    .when(status.additions > 0, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "titlebar-git-additions".into())
                                .text_color(success)
                                .child(format!("+{}", status.additions)),
                        )
                    })
                    .when(status.deletions > 0, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "titlebar-git-deletions".into())
                                .text_color(danger)
                                .child(format!("-{}", status.deletions)),
                        )
                    })
                    // Untracked files are staged by a commit too, but have no
                    // diff against HEAD.
                    .when(status.untracked > 0, |row| {
                        row.child(
                            div()
                                .debug_selector(|| "titlebar-git-untracked".into())
                                .text_color(muted)
                                .child(if status.additions == 0 && status.deletions == 0 {
                                    "Uncommitted"
                                } else {
                                    "*"
                                }),
                        )
                    })
                    .into_any_element()
            }),
        };
        let button = Button::new("titlebar-git")
            .debug_selector(|| "titlebar-git".into())
            .ghost()
            .xsmall()
            .icon(
                Icon::empty()
                    .path("icons/git-branch.svg")
                    .text_color(if running { warning } else { muted }),
            )
            .dropdown_caret(true)
            .tooltip("Git")
            // Opens on press, as a menu button does, anchored at the pointer.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_git_menu(event.position, window, cx);
                }),
            );
        // The pull request's churn is history; the dot says work is still
        // sitting in the checkout.
        let button = if pr.is_some() && dirty {
            Badge::new()
                .dot()
                .color(warning)
                .child(button)
                .into_any_element()
        } else {
            button.into_any_element()
        };
        Some(
            h_flex()
                .gap_1()
                .flex_none()
                .children(summary)
                .child(button)
                .into_any_element(),
        )
    }

    fn render_account_button(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let image = self
            .menu
            .github
            .profile
            .as_ref()
            .and_then(|profile| profile.avatar.clone());
        let login: Option<SharedString> = self
            .menu
            .github
            .profile
            .as_ref()
            .map(|profile| profile.login.clone().into());
        Button::new("titlebar-avatar")
            .debug_selector(|| "titlebar-avatar".into())
            .ghost()
            .small()
            .compact()
            .tooltip(login.unwrap_or_else(|| "GitHub".into()))
            .child(
                Avatar::new()
                    .with_size(px(20.))
                    .placeholder(IconName::Github)
                    .when_some(image, |avatar, image| avatar.src(image)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.open_profile(true, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_profile(false, window, cx);
                }),
            )
    }

    pub(super) fn render_titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        bar(h_flex()
            .flex_1()
            .h_full()
            .justify_end()
            .gap_1()
            .pr_1()
            .children(self.render_git_button(cx))
            .child(self.render_account_button(cx)))
    }
}

/// The window's title bar with `content` at its trailing end. On macOS and
/// Linux the kit title bar owns dragging and double-click zoom; Windows keeps
/// its native caption (see [`options`]), where a kit bar would stack a second
/// set of window controls under it, so only a plain row is drawn there.
#[derive(IntoElement)]
pub(super) struct Bar(AnyElement);

pub(super) fn bar(content: impl IntoElement) -> Bar {
    Bar(content.into_any_element())
}

impl RenderOnce for Bar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .debug_selector(|| "titlebar".into())
            .flex_none()
            .w_full()
            .map(|slot| {
                if cfg!(target_os = "windows") {
                    slot.child(
                        h_flex()
                            .h(TITLE_BAR_HEIGHT)
                            .border_b_1()
                            .border_color(cx.theme().title_bar_border)
                            .bg(cx.theme().title_bar)
                            .child(self.0),
                    )
                } else {
                    slot.child(TitleBar::new().child(self.0))
                }
            })
    }
}

pub(super) fn options(title: &str) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title.to_owned().into()),
        appears_transparent: cfg!(target_os = "macos"),
        traffic_light_position: cfg!(target_os = "macos").then(|| point(px(9.), px(9.))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use crate::menu::Page;
    use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, TestAppContext, px, size};

    #[gpui_kit::test]
    fn profile_context_menu_does_not_start_auth(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(1200.), px(780.)));
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let bounds = cx.debug_bounds("titlebar-avatar").unwrap();
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Right,
            position: bounds.center(),
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.menu.page == Some(Page::GitHub));
            assert!(!view.menu.github.busy());
            assert!(!view.menu.github.loading_profile());
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.dismiss_menu(window, cx);
                view.menu.github = crate::github::Auth::connected_fixture();
                view.open_profile(true, window, cx);
                assert!(view.menu.github.connected());
                assert!(!view.menu.github.busy());
            })
        });
    }
}

#[cfg(test)]
mod git_button_tests {
    #![allow(clippy::unwrap_used)]
    use crate::{
        git::{Git, Status},
        menu::Page,
        pull_request::Input,
        sidebar::layout_tests::REPO_KEY,
    };
    use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, TestAppContext, px, size};

    fn draw(cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    fn input() -> Input {
        Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "develop".into(),
        }
    }

    #[gpui_kit::test]
    fn git_button_appears_for_a_tracked_checkout_and_opens_its_menu(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(1200.), px(600.)));
        cx.run_until_parked();
        draw(cx);
        assert!(
            cx.debug_bounds("titlebar-git").is_none(),
            "no tracked checkout, no Git control"
        );
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.git = Git::fixture(
                    input(),
                    Status {
                        additions: 239,
                        deletions: 250,
                        untracked: 2,
                    },
                );
                cx.notify();
            })
        });
        draw(cx);
        let button = cx.debug_bounds("titlebar-git").unwrap();
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: button.center(),
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::Git)));
    }

    #[gpui_kit::test]
    fn uncommitted_changes_hide_zero_counts(cx: &mut TestAppContext) {
        for (additions, deletions, untracked) in [(0, 0, 1), (12, 0, 0), (0, 3, 0), (0, 0, 0)] {
            let (view, cx) = crate::test_support::add_window_view(
                cx,
                crate::sidebar::layout_tests::fixture_window,
            );
            cx.simulate_resize(size(px(900.), px(600.)));
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.git = Git::fixture(
                        input(),
                        Status {
                            additions,
                            deletions,
                            untracked,
                        },
                    );
                    cx.notify();
                });
            });
            cx.run_until_parked();
            draw(cx);
            assert!(cx.debug_bounds("titlebar-git").is_some());
            assert_eq!(
                cx.debug_bounds("titlebar-git-additions").is_some(),
                additions > 0
            );
            assert_eq!(
                cx.debug_bounds("titlebar-git-deletions").is_some(),
                deletions > 0
            );
            assert_eq!(
                cx.debug_bounds("titlebar-git-untracked").is_some(),
                untracked > 0
            );
            assert!(
                cx.debug_bounds("titlebar-git-pr").is_none(),
                "no cached pull request, no pull request link"
            );
        }
    }

    #[gpui_kit::test]
    fn a_cached_pull_request_replaces_the_uncommitted_counts(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.git = Git::fixture(
                    input(),
                    Status {
                        additions: 12,
                        deletions: 3,
                        untracked: 0,
                    },
                );
                view.menu.github = crate::github::Auth::connected_fixture();
                view.menu.pr_cache.seed(
                    input(),
                    crate::pull_request::fixture().unwrap(),
                    std::time::Instant::now(),
                );
                cx.notify();
            })
        });
        draw(cx);
        // One set of counts only: the pull request's. Two "+N -M" pairs never
        // sit together.
        assert!(cx.debug_bounds("titlebar-git-pr").is_some());
        assert!(
            cx.debug_bounds("titlebar-git-additions").is_none(),
            "uncommitted counts give way to the pull request's"
        );
        let link = cx.debug_bounds("titlebar-git-pr-link").unwrap();
        cx.simulate_click(link.center(), Modifiers::default());
        assert_eq!(
            cx.opened_url(),
            Some(crate::pull_request::fixture().unwrap().url)
        );
        cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, None));
    }
}

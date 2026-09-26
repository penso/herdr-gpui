//! Native chrome and GitHub account access.
use crate::{HerdrWindow, fonts::StyledFont, menu::Page};
use gpui::{prelude::*, *};

/// Avatar or signed-out GitHub icon. Smaller than the hit target, which stays a
/// comfortable size for the pointer.
const AVATAR: f32 = 20.;

/// Native chrome the window draws above its body; popups must clear it.
pub(super) const HEIGHT: f32 = 34.;

impl HerdrWindow {
    fn open_profile(&mut self, connect: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.page != Some(Page::GitHub) && !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::GitHub);
        // A device already covered by the main account opens the page rather
        // than starting a second sign-in for itself.
        if connect && self.pr_profile().is_none() && !self.github_auth().loading_profile() {
            self.start_github();
        }
        cx.notify();
    }

    /// Git actions for the focused checkout, left of the account slot. Hidden
    /// when no local checkout is tracked, so remote endpoints show no control
    /// that cannot act.
    ///
    /// One set of counts only, so two "+N -M" pairs can never sit side by side
    /// meaning different things. A branch with a prefetched pull request shows
    /// that pull request, exactly as its sidebar row does, and a badge when the
    /// checkout also has uncommitted work; the popup says how much. A branch
    /// without one shows what a commit would include right now.
    fn render_git_button(&self, cx: &mut Context<Self>) -> Option<Div> {
        self.git.tracked()?;
        let readable_theme = self.theme.readable_chrome(self.paint_opacity());
        let theme = &readable_theme;
        let font = &self.config.ui;
        let background = rgb(theme.surface).blend(rgba(0xffffff1a));
        let status = self.git.status();
        let running = self.git.running().is_some();
        let pr = self.git_pull_request().map(|pr| {
            (
                format!("#{}", pr.number),
                pr.color(theme),
                pr.additions,
                pr.deletions,
                pr.url.clone(),
            )
        });
        Some(
            div()
                .debug_selector(|| "titlebar-git-slot".into())
                .flex()
                .items_center()
                .flex_none()
                .h_full()
                .pr(px(2.))
                .gap(px(4.))
                .text_font(font)
                .text_size(px(font.size))
                .text_color(rgb(theme.foreground))
                .map(|button| match pr {
                    Some((number, color, additions, deletions, url)) => button
                        .child(
                            div()
                                .id("titlebar-git-pr-link")
                                .debug_selector(|| "titlebar-git-pr-link".into())
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .h(px(24.))
                                .px(px(6.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .cursor_pointer()
                                .hover(|link| {
                                    link.bg(background.blend(rgba((theme.foreground << 8) | 0x14)))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    cx.open_url(&url);
                                })
                                .child(
                                    div()
                                        .debug_selector(|| "titlebar-git-pr".into())
                                        .text_color(rgb(color))
                                        .child(number),
                                )
                                .child(
                                    div()
                                        .debug_selector(|| "titlebar-git-pr-lines".into())
                                        .flex()
                                        .child(
                                            div()
                                                .debug_selector(|| {
                                                    "titlebar-git-pr-additions".into()
                                                })
                                                .text_color(rgb(theme.palette[2]))
                                                .child(format!(
                                                    "+{}",
                                                    crate::sidebar::compact(additions)
                                                )),
                                        )
                                        .child(div().text_color(rgb(theme.muted)).child("/"))
                                        .child(
                                            div()
                                                .debug_selector(|| {
                                                    "titlebar-git-pr-deletions".into()
                                                })
                                                .text_color(rgb(theme.palette[1]))
                                                .child(format!(
                                                    "-{}",
                                                    crate::sidebar::compact(deletions)
                                                )),
                                        ),
                                ),
                        )
                        // The pull request's churn is history; the badge
                        // says work is still sitting in the checkout.
                        .when(status.is_some_and(|status| status.dirty()), |button| {
                            button.child(
                                crate::icons::uncommitted(theme, 18.)
                                    .debug_selector(|| "titlebar-git-dirty".into()),
                            )
                        }),
                    None => button.when_some(
                        status.filter(|status| status.dirty()),
                        |button, status| {
                            button
                                .when(status.additions > 0, |button| {
                                    button.child(
                                        div()
                                            .debug_selector(|| "titlebar-git-additions".into())
                                            .text_color(rgb(theme.palette[2]))
                                            .child(format!("+{}", status.additions)),
                                    )
                                })
                                .when(status.deletions > 0, |button| {
                                    button.child(
                                        div()
                                            .debug_selector(|| "titlebar-git-deletions".into())
                                            .text_color(rgb(theme.palette[1]))
                                            .child(format!("-{}", status.deletions)),
                                    )
                                })
                                // Untracked files are staged by a commit
                                // too, but have no diff against HEAD.
                                .when(status.untracked > 0, |button| {
                                    button.child(
                                        div()
                                            .debug_selector(|| "titlebar-git-untracked".into())
                                            .text_color(rgb(theme.muted))
                                            .child(
                                                if status.additions == 0 && status.deletions == 0 {
                                                    "Uncommitted"
                                                } else {
                                                    "*"
                                                },
                                            ),
                                    )
                                })
                        },
                    ),
                })
                .child(
                    div()
                        .id("titlebar-git")
                        .debug_selector(|| "titlebar-git".into())
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .h(px(24.))
                        .px(px(6.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .cursor_pointer()
                        .hover(|button| {
                            button.bg(background.blend(rgba((theme.foreground << 8) | 0x14)))
                        })
                        .child(
                            svg()
                                .path("icons/git-branch.svg")
                                .size(px(14.))
                                .flex_none()
                                .text_color(rgb(if running {
                                    theme.palette[3]
                                } else {
                                    theme.muted
                                })),
                        )
                        .child(
                            svg()
                                .path("icons/chevron-down.svg")
                                .size(px(12.))
                                .flex_none()
                                .text_color(rgb(theme.muted)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.open_git_menu(event.position, window, cx);
                            }),
                        ),
                ),
        )
    }

    pub(super) fn render_titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.pr_profile().and_then(|p| p.avatar.clone());
        render(self.theme.surface, self.paint_opacity())
            .children(self.render_git_button(cx))
            .child(
                div()
                    .debug_selector(|| "titlebar-account-slot".into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_none()
                    .w(px(40.))
                    .h_full()
                    .child(
                        div()
                            .id("titlebar-avatar")
                            .group("titlebar-account")
                            .debug_selector(|| "titlebar-avatar".into())
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(28.))
                            .rounded_full()
                            .cursor_pointer()
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
                            .child(
                                div()
                                    .debug_selector(|| "titlebar-avatar-circle".into())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(AVATAR))
                                    .rounded_full()
                                    .group_hover("titlebar-account", |s| {
                                        s.shadow(vec![BoxShadow {
                                            color: rgba((self.theme.foreground << 8) | 0x38).into(),
                                            offset: point(px(0.), px(0.)),
                                            blur_radius: px(5.),
                                            spread_radius: px(1.),
                                            inset: false,
                                        }])
                                    })
                                    .map(|circle| match image {
                                        Some(image) => {
                                            circle.child(img(image).size(px(AVATAR)).rounded_full())
                                        }
                                        None => circle.child(
                                            svg()
                                                .path("icons/github.svg")
                                                .size(px(AVATAR))
                                                .text_color(rgb(self.theme.foreground)),
                                        ),
                                    }),
                            ),
                    ),
            )
    }
}

pub(super) fn render(surface: u32, opacity: u8) -> Stateful<Div> {
    // AppKit owns dragging; GPUI's macOS backend cannot start a custom move.
    div()
        .id("titlebar")
        .debug_selector(|| "titlebar".into())
        .flex()
        .flex_none()
        .w_full()
        .h(px(HEIGHT))
        .bg(crate::config::background(surface, opacity))
        .child(div().flex_none().w(px(80.)).h_full())
        .child(
            div()
                .debug_selector(|| "titlebar-center".into())
                .flex_1()
                .min_w_0()
                .h_full(),
        )
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.titlebar_double_click();
            }
        })
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
    use gpui::{Bounds, Modifiers, MouseButton, MouseDownEvent, TestAppContext, point, px, size};

    #[gpui::test]
    fn account_icon_keeps_the_same_bounds_when_signed_out_failed_or_connected(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        for width in [1200., 360.] {
            cx.simulate_resize(size(px(width), px(400.)));
            for state in 0..4 {
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.menu.github = match state {
                            2 => crate::github::Auth::fixture(true),
                            3 => crate::github::Auth::connected_fixture(),
                            _ => crate::github::Auth::default(),
                        };
                        view.menu.github.failed = state == 1;
                        if let Some(profile) = view.menu.github.profile.as_mut() {
                            profile.avatar = Some(std::sync::Arc::new(gpui::Image::from_bytes(
                                gpui::ImageFormat::Svg,
                                include_bytes!("../../../assets/icons/user.svg").to_vec(),
                            )));
                        }
                        cx.notify();
                    });
                });
                for hovered in [false, true] {
                    cx.update(|window, cx| {
                        window.refresh();
                        let _ = window.draw(cx);
                    });
                    let hit = cx.debug_bounds("titlebar-avatar").unwrap();
                    cx.simulate_event(gpui::MouseMoveEvent {
                        position: if hovered {
                            hit.center()
                        } else {
                            point(px(100.), px(100.))
                        },
                        ..Default::default()
                    });
                    cx.update(|window, cx| {
                        window.refresh();
                        let _ = window.draw(cx);
                    });
                    let icon = cx.debug_bounds("titlebar-avatar-circle").unwrap();
                    assert_eq!(icon.size, size(px(super::AVATAR), px(super::AVATAR)));
                    assert_eq!(icon.center(), hit.center());
                    assert_eq!(hit.size, size(px(28.), px(28.)));
                }
            }
        }
    }

    #[gpui::test]
    fn profile_slot_bounds_and_context_menu_do_not_start_auth(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        for (width, height) in [(1200., 780.), (640., 400.), (360., 400.)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            assert_eq!(
                cx.debug_bounds("titlebar").unwrap(),
                Bounds::new(point(px(0.), px(0.)), size(px(width), px(34.)))
            );
            assert_eq!(
                cx.debug_bounds("titlebar-avatar").unwrap(),
                Bounds::new(point(px(width - 34.), px(3.)), size(px(28.), px(28.)))
            );
            let banner_height = if env!("HERDR_BUILD_WORKTREE") == "1" {
                22.
            } else {
                0.
            };
            assert_eq!(
                cx.debug_bounds("window-body").unwrap().top(),
                px(34. + banner_height)
            );
        }
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
    use gpui::{Modifiers, MouseButton, MouseDownEvent, TestAppContext, px, size};

    #[gpui::test]
    fn git_button_sits_left_of_the_account_slot_and_opens_its_menu(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
        };
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
                    Input {
                        checkout: None,
                        repo_key: REPO_KEY.into(),
                        branch: "develop".into(),
                    },
                    Status {
                        additions: 239,
                        deletions: 250,
                        untracked: 2,
                    },
                );
                cx.notify();
            })
        });
        for width in [1200., 640., 360.] {
            cx.simulate_resize(size(px(width), px(600.)));
            cx.run_until_parked();
            draw(cx);
            let button = cx.debug_bounds("titlebar-git").unwrap();
            let avatar = cx.debug_bounds("titlebar-avatar").unwrap();
            let titlebar = cx.debug_bounds("titlebar").unwrap();
            assert!(button.right() <= avatar.left(), "width {width}");
            assert!(button.left() >= titlebar.left());
            assert!(button.top() >= titlebar.top() && button.bottom() <= titlebar.bottom());
            for part in [
                "titlebar-git-additions",
                "titlebar-git-deletions",
                "titlebar-git-untracked",
            ] {
                let bounds = cx.debug_bounds(part).unwrap();
                assert!(bounds.right() <= button.left(), "{part} at width {width}");
            }
            cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position: button.center(),
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            });
            cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::Git)));
            draw(cx);
            for row in [
                "git-menu-Commit...",
                "git-menu-Push",
                "git-menu-Create pull request",
            ] {
                assert!(cx.debug_bounds(row).is_some(), "{row} at width {width}");
            }
            assert!(cx.debug_bounds("git-menu-summary").is_some());
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_menu(window, cx);
                    cx.notify();
                })
            });
        }
    }

    #[gpui::test]
    fn uncommitted_changes_hide_zero_counts(cx: &mut TestAppContext) {
        for (additions, deletions, untracked) in [(0, 0, 1), (12, 0, 0), (0, 3, 0)] {
            let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
            cx.simulate_resize(size(px(360.), px(600.)));
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.git = Git::fixture(
                        Input {
                            checkout: None,
                            repo_key: REPO_KEY.into(),
                            branch: "develop".into(),
                        },
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
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
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
            let button = cx.debug_bounds("titlebar-git").unwrap();
            assert!(button.left() >= cx.debug_bounds("titlebar").unwrap().left());
            assert!(button.right() <= cx.debug_bounds("titlebar-avatar").unwrap().left());
        }
    }

    #[gpui::test]
    fn a_clean_checkout_shows_no_counts(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.git = Git::fixture(
                    Input {
                        checkout: None,
                        repo_key: REPO_KEY.into(),
                        branch: "develop".into(),
                    },
                    Status::default(),
                );
                cx.notify();
            })
        });
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("titlebar-git").is_some());
        assert!(cx.debug_bounds("titlebar-git-additions").is_none());
        assert!(cx.debug_bounds("titlebar-git-untracked").is_none());
        assert!(
            cx.debug_bounds("titlebar-git-pr").is_none(),
            "no cached pull request, no badge"
        );
    }

    #[gpui::test]
    fn a_cached_pull_request_replaces_the_uncommitted_counts(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let input = Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "develop".into(),
        };
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.git = Git::fixture(
                    input.clone(),
                    Status {
                        additions: 12,
                        deletions: 3,
                        untracked: 0,
                    },
                );
                view.menu.github = crate::github::Auth::connected_fixture();
                view.menu.pr_cache.seed(
                    input,
                    crate::pull_request::fixture().unwrap(),
                    std::time::Instant::now(),
                );
                cx.notify();
            })
        });
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let button = cx.debug_bounds("titlebar-git").unwrap();
        let number = cx.debug_bounds("titlebar-git-pr").unwrap();
        let churn = cx.debug_bounds("titlebar-git-pr-lines").unwrap();
        let dirty = cx.debug_bounds("titlebar-git-dirty").unwrap();
        assert_eq!(dirty.size, size(px(18.), px(18.)));
        // One set of counts only: the pull request's, then a dot for the work
        // still sitting in the checkout. Two "+N -M" pairs never sit together.
        assert!(
            cx.debug_bounds("titlebar-git-additions").is_none(),
            "uncommitted counts give way to the pull request's"
        );
        assert!(number.right() <= churn.left());
        assert!(churn.right() <= dirty.left());
        assert!(dirty.right() <= button.left());
        assert!(button.right() <= cx.debug_bounds("titlebar-avatar").unwrap().left());
        // Additions and deletions are separate spans so each keeps its own
        // color, as the sidebar badge paints them.
        let additions = cx.debug_bounds("titlebar-git-pr-additions").unwrap();
        let deletions = cx.debug_bounds("titlebar-git-pr-deletions").unwrap();
        assert!(churn.left() <= additions.left() && additions.right() <= deletions.left());
        assert!(deletions.right() <= churn.right());
        for width in [900., 360.] {
            cx.simulate_resize(size(px(width), px(600.)));
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            let number = cx.debug_bounds("titlebar-git-pr").unwrap();
            let churn = cx.debug_bounds("titlebar-git-pr-lines").unwrap();
            let button = cx.debug_bounds("titlebar-git").unwrap();
            assert!(number.left() >= px(80.));
            assert!(number.right() <= churn.left());
            assert!(churn.right() <= button.left());
            for selector in [
                "titlebar-git-pr",
                "titlebar-git-pr-additions",
                "titlebar-git-pr-deletions",
                "titlebar-git-pr-link",
            ] {
                cx.update(|_, cx| cx.open_url("https://example.com"));
                let target = cx.debug_bounds(selector).unwrap();
                cx.simulate_click(target.center(), Modifiers::default());
                assert_eq!(
                    cx.opened_url(),
                    Some(crate::pull_request::fixture().unwrap().url),
                    "{selector} should open the PR"
                );
                cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, None));
            }
            cx.simulate_click(button.center(), Modifiers::default());
            cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::Git)));
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.dismiss_menu(window, cx));
            });
        }
    }

    #[gpui::test]
    fn a_clean_checkout_with_a_pull_request_shows_no_dot(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let input = Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "develop".into(),
        };
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.git = Git::fixture(input.clone(), Status::default());
                view.menu.github = crate::github::Auth::connected_fixture();
                view.menu.pr_cache.seed(
                    input,
                    crate::pull_request::fixture().unwrap(),
                    std::time::Instant::now(),
                );
                cx.notify();
            })
        });
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("titlebar-git-pr-lines").is_some());
        assert!(cx.debug_bounds("titlebar-git-dirty").is_none());
    }
}

#[cfg(all(test, target_os = "macos"))]
#[allow(clippy::unwrap_used)]
mod native_chrome_tests {
    use gpui::{Bounds, TestAppContext, point, px, size};

    #[gpui::test]
    fn header_bounds_above_body_in_windowed_and_fullscreen(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        for fullscreen in [false, true, false] {
            cx.update(|window, _| {
                if window.is_fullscreen() != fullscreen {
                    window.toggle_fullscreen();
                }
                assert_eq!(window.is_fullscreen(), fullscreen);
            });
            for (width, height) in [(1200., 780.), (640., 400.), (360., 400.)] {
                cx.simulate_resize(size(px(width), px(height)));
                cx.run_until_parked();
                cx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
                assert_eq!(
                    cx.debug_bounds("titlebar").unwrap(),
                    Bounds::new(point(px(0.), px(0.)), size(px(width), px(34.)))
                );
                assert_eq!(
                    cx.debug_bounds("titlebar-account-slot").unwrap(),
                    Bounds::new(point(px(width - 40.), px(0.)), size(px(40.), px(34.)))
                );
                assert_eq!(
                    cx.debug_bounds("titlebar-avatar").unwrap(),
                    Bounds::new(point(px(width - 34.), px(3.)), size(px(28.), px(28.)))
                );
                assert_eq!(
                    cx.debug_bounds("titlebar-avatar-circle").unwrap(),
                    Bounds::new(point(px(width - 30.), px(7.)), size(px(20.), px(20.)))
                );
                // Signing in swaps the placeholder for the avatar, which sits
                // inside the same hit target rather than filling it.
                let view =
                    cx.update(|window, _| window.root::<crate::HerdrWindow>().unwrap().unwrap());
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.menu.github = crate::github::Auth::connected_fixture();
                        if let Some(profile) = view.menu.github.profile.as_mut() {
                            profile.avatar = Some(std::sync::Arc::new(gpui::Image::from_bytes(
                                gpui::ImageFormat::Svg,
                                include_bytes!("../../../assets/icons/user.svg").to_vec(),
                            )));
                        }
                        cx.notify();
                    })
                });
                cx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
                let hit = cx.debug_bounds("titlebar-avatar").unwrap();
                let circle = cx.debug_bounds("titlebar-avatar-circle").unwrap();
                assert_eq!(circle.size, size(px(super::AVATAR), px(super::AVATAR)));
                assert_eq!(circle.center(), hit.center());
                assert!(circle.size.width < hit.size.width);
                cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.menu.github = Default::default();
                        cx.notify();
                    })
                });
                cx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
                assert_eq!(
                    cx.debug_bounds("titlebar-center").unwrap(),
                    Bounds::new(point(px(80.), px(0.)), size(px(width - 120.), px(34.)))
                );
                let body = cx.debug_bounds("window-body").unwrap();
                let banner_height = if env!("HERDR_BUILD_WORKTREE") == "1" {
                    22.
                } else {
                    0.
                };
                assert_eq!(body.top(), px(34. + banner_height));
                assert_eq!(body.size.width, px(width));
                assert!(body.bottom() <= px(height));
            }
        }
    }
}

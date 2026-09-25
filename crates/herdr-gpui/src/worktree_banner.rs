//! The strip that marks a worktree build, so it is never mistaken for a
//! release: the branch it was built from and, when there is one, its pull
//! request.
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, link::Link, tag::Tag};
use gpui_kit::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    div, prelude::FluentBuilder, px,
};

/// Worktree builds hang this strip under the titlebar; other builds do not.
/// Fixed, so popups that clear the chrome can reserve it.
const HEIGHT: f32 = 22.;

pub(super) fn reserved(worktree: bool) -> f32 {
    if worktree { HEIGHT } else { 0. }
}

pub(super) fn pull_request_url(pr: &str) -> String {
    format!("{}/pull/{pr}", crate::about::REPOSITORY)
}

#[derive(IntoElement)]
pub(super) struct Banner {
    branch: SharedString,
    pr: Option<SharedString>,
}

pub(super) fn render(worktree: bool, branch: &str, pr: &str) -> Option<Banner> {
    worktree.then(|| Banner {
        branch: branch.to_owned().into(),
        pr: (!pr.is_empty()).then(|| pr.to_owned().into()),
    })
}

impl RenderOnce for Banner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .debug_selector(|| "worktree-banner".into())
            .flex_none()
            .w_full()
            .h(px(HEIGHT))
            .gap_2()
            .px_3()
            .overflow_hidden()
            .text_xs()
            .bg(cx.theme().warning.opacity(0.15))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(Tag::warning().xsmall().flex_none().child("Worktree"))
            .child(
                div()
                    .debug_selector(|| "worktree-branch".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.branch),
            )
            .when_some(self.pr, |row, pr| {
                row.child(
                    div()
                        .debug_selector(|| "worktree-pr".into())
                        .flex_none()
                        .child(
                            Link::new("worktree-pr")
                                .href(pull_request_url(&pr))
                                .child(format!("PR #{pr}")),
                        ),
                )
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::render;
    use gpui_kit::{
        Context, IntoElement, Render, TestAppContext, Window, div, prelude::*, px, size,
    };

    struct Fixture {
        worktree: bool,
        pr: &'static str,
    }

    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .children(render(
                    self.worktree,
                    "worktree/a-very-long-branch-name-that-must-not-push-the-pr-out-of-the-window",
                    self.pr,
                ))
                .child(div().debug_selector(|| "body".into()).flex_1().min_h_0())
        }
    }

    #[gpui_kit::test]
    fn banner_reserves_space_only_for_worktrees(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |_, _| Fixture {
            worktree: false,
            pr: "",
        });
        for worktree in [false, true] {
            for pr in ["", "12345"] {
                for width in [360., 640., 1200.] {
                    view.update(cx, |view, cx| {
                        view.worktree = worktree;
                        view.pr = pr;
                        cx.notify();
                    });
                    cx.simulate_resize(size(px(width), px(400.)));
                    cx.run_until_parked();
                    cx.update(|window, cx| {
                        window.refresh();
                        let _ = window.draw(cx);
                    });
                    let body = cx.debug_bounds("body").unwrap();
                    assert_eq!(body.top(), px(if worktree { 22. } else { 0. }));
                    assert_eq!(body.bottom(), px(400.));
                    if worktree {
                        let banner = cx.debug_bounds("worktree-banner").unwrap();
                        let branch = cx.debug_bounds("worktree-branch").unwrap();
                        assert_eq!(banner.size.height, px(22.));
                        assert!(branch.right() <= banner.right());
                        if !pr.is_empty() {
                            let pr = cx.debug_bounds("worktree-pr").unwrap();
                            assert!(pr.left() >= branch.right());
                            assert!(pr.right() <= banner.right());
                            cx.simulate_click(pr.center(), Default::default());
                            assert_eq!(
                                cx.opened_url().as_deref(),
                                Some("https://github.com/penso/herdr-gpui/pull/12345")
                            );
                        } else {
                            assert!(cx.debug_bounds("worktree-pr").is_none());
                        }
                    } else {
                        assert!(cx.debug_bounds("worktree-banner").is_none());
                    }
                }
            }
        }
    }
}

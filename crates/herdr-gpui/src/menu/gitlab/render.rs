//! Drawing the account menu's header, shared by its GitHub and GitLab pages,
//! and the GitLab page itself.

use super::{HostStatus, Token, connect::Connecting, keys::Action, status_line};
use crate::{HerdrWindow, fonts::StyledFont, menu::Page, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_orchestrator::gitlab::account::Source;

/// The official GitLab mark, drawn in one color as the GitHub mark is.
pub(crate) fn gitlab_mark(color: u32) -> Svg {
    svg()
        .path("icons/gitlab.svg")
        .flex_none()
        .text_color(rgb(color))
}

impl HerdrWindow {
    /// The account menu's header: a tab for each provider, the shown one
    /// marked, and Close.
    pub(in crate::menu) fn render_account_tabs(&self, page: Page, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let tab = |id: &'static str, label: &'static str, mark: Svg, shown: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Tab)
                .aria_label(label)
                .aria_selected(shown)
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .when(shown, |tab| tab.bg(rgb(theme.active)))
                .hover(|style| style.bg(rgb(theme.active)))
                .child(mark.size(px(20.)))
                .child(
                    div()
                        .text_size(px(font.size * 1.15))
                        .font_weight(if shown {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(rgb(if shown { theme.foreground } else { theme.muted }))
                        .child(label),
                )
        };
        let github_color = if page == Page::GitHub {
            theme.foreground
        } else {
            theme.muted
        };
        let gitlab_color = if page == Page::GitLab {
            theme.foreground
        } else {
            theme.muted
        };
        div()
            .flex()
            .items_center()
            .flex_none()
            .gap(px(4.))
            .p(px(10.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                tab(
                    "accounts-tab-github",
                    "GitHub",
                    crate::sidebar::github_mark(github_color)
                        .debug_selector(|| "github-mark".into()),
                    page == Page::GitHub,
                )
                .when(
                    page == Page::GitLab && self.menu.gitlab.is_selected(&Action::GitHub),
                    |tab| tab.bg(rgb(theme.active)),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.menu.page = Some(Page::GitHub);
                    cx.notify();
                })),
            )
            .child(
                tab(
                    "accounts-tab-gitlab",
                    "GitLab",
                    gitlab_mark(gitlab_color),
                    page == Page::GitLab,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    if this.menu.page != Some(Page::GitLab) {
                        this.open_gitlab_page(window, cx);
                    }
                })),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("github-header-close")
                    .debug_selector(|| "github-header-close".into())
                    .role(Role::Button)
                    .aria_label("Close")
                    .cursor_pointer()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .when(
                        (page == Page::GitHub
                            && self.menu.github_selected
                                == Some(crate::menu::github::Action::Close))
                            || (page == Page::GitLab
                                && self.menu.gitlab.is_selected(&Action::Close)),
                        |style| style.bg(rgb(theme.active)),
                    )
                    .hover(|style| style.bg(rgb(theme.active)))
                    .child(crate::sidebar::label_text("Close"))
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx))),
            )
    }

    fn gitlab_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        primary: bool,
        enabled: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let accent = crate::menu::accent(theme);
        let id: SharedString = id.into();
        let label: SharedString = label.into();
        div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .role(Role::Button)
            .aria_label(label.clone())
            .flex_none()
            .px(px(12.))
            .py(px(6.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .when(primary, |button| {
                button
                    .bg(accent)
                    .text_color(rgb(theme.background))
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .when(!enabled, |button| button.opacity(0.5))
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.border_color(accent))
            })
            .child(crate::sidebar::label_text(&label))
    }

    /// How the control the keyboard selected shows, on a filled button too.
    fn selected_look(&self, button: Stateful<Div>) -> Stateful<Div> {
        button.border_color(rgb(self.theme.foreground))
    }

    /// An instance's card: the account's picture and name, the instance with
    /// where its token comes from, and what can be done about it.
    fn render_gitlab_host(&self, index: usize, status: &HostStatus, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let gitlab = &self.menu.gitlab;
        let user = match &status.token {
            Token::Found {
                user: Some(Ok(user)),
                ..
            } => Some(user),
            _ => None,
        };
        let picture = user
            .and_then(|user| user.avatar_url.as_deref())
            .and_then(|url| gitlab.avatars.get(url));
        let source = match &status.token {
            Token::Found { source, .. } => Some(source_chip(*source, status)),
            Token::None if !status.plain => Some("no token"),
            _ => None,
        };
        let (title, detail) = match (user, &status.token) {
            (Some(user), _) => (format!("@{}", user.username), None),
            (None, Token::None) if !status.plain => ("Not connected".to_owned(), None),
            _ => (status.host.clone(), Some(status_line(status))),
        };
        let mut chip = source.map(|label| {
            div()
                .debug_selector(move || format!("gitlab-source-{index}"))
                .flex_none()
                .px(px(6.))
                .rounded(px(crate::config::corners::SMALL))
                .bg(rgb(theme.active))
                .text_color(rgb(theme.subtext()))
                .text_size(px(self.config.ui.size - 1.))
                .child(label)
        });
        let connecting = gitlab
            .connecting
            .as_ref()
            .filter(|connecting| connecting.host == status.host);
        let actions = self.render_gitlab_actions(index, status, cx);
        div()
            .debug_selector(move || format!("gitlab-host-{index}"))
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(12.))
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.background))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .debug_selector(move || format!("gitlab-avatar-{index}"))
                            .flex_none()
                            .size(px(36.))
                            .rounded_full()
                            .overflow_hidden()
                            .child(match picture {
                                // Round the image itself: GPUI clips overflow
                                // to the box, not to its corner radii.
                                Some(image) => {
                                    img(image).size_full().rounded_full().into_any_element()
                                }
                                None => gitlab_mark(theme.muted).size_full().into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(title),
                                    )
                                    // With an explanation below, the chip
                                    // rides the title line.
                                    .when(detail.is_some(), |line| line.children(chip.take())),
                            )
                            .when(detail.is_none(), |column| {
                                column.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .min_w_0()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_color(rgb(theme.muted))
                                                .child(status.host.clone()),
                                        )
                                        .children(chip.take()),
                                )
                            })
                            .children(detail.map(|detail| {
                                div().text_color(rgb(theme.subtext())).child(detail)
                            })),
                    )
                    .child(actions),
            )
            .children(connecting.map(|connecting| self.render_gitlab_connecting(connecting, cx)))
    }

    /// A card's buttons: Connect where an application can sign in and no
    /// account is, or Connect again where GitLab refused the connected one,
    /// Disconnect or Forget for what is saved here, else the checkout whose
    /// `.envrc` gives the token.
    fn render_gitlab_actions(
        &self,
        index: usize,
        status: &HostStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let gitlab = &self.menu.gitlab;
        let checking = gitlab.checking.contains(&status.host);
        let ready = !gitlab.working && gitlab.connecting.is_none() && !checking;
        let host = status.host.clone();
        let (connect, remove) = super::keys::card_buttons(status);
        let connect = connect.map(|label| {
            self.gitlab_connect_button(
                format!("gitlab-connect-{index}"),
                label,
                &host,
                checking,
                cx,
            )
        });
        let remove = remove.map(|label| {
            let selected = gitlab.is_selected(&Action::Remove(host.clone()));
            let host = host.clone();
            self.gitlab_button(format!("gitlab-forget-{index}"), label, false, ready)
                .when(selected, |button| self.selected_look(button))
                .when(ready, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.forget_gitlab(host.clone(), cx);
                    }))
                })
        });
        let from = (connect.is_none() && remove.is_none())
            .then(|| match (&status.token, &status.checkout) {
                (
                    Token::Found {
                        source: Source::Envrc,
                        ..
                    },
                    Some(checkout),
                ) => Some(format!("from {checkout}/")),
                _ => None,
            })
            .flatten();
        div()
            .flex_none()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(6.))
            .children(connect)
            .children(remove)
            .children(from.map(|from| {
                div()
                    .max_w(px(120.))
                    .truncate()
                    .text_color(rgb(self.theme.muted))
                    .child(from)
            }))
    }

    fn gitlab_connect_button(
        &self,
        id: String,
        label: impl Into<SharedString>,
        host: &str,
        checking: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let gitlab = &self.menu.gitlab;
        let ready = !gitlab.working && gitlab.connecting.is_none() && !checking;
        let host = host.to_owned();
        let selected = gitlab.is_selected(&Action::Connect(host.clone()));
        self.gitlab_button(id, label, true, ready)
            .when(selected, |button| self.selected_look(button))
            .when(ready, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.connect_gitlab(host.clone(), cx);
                }))
            })
    }

    /// The page when no instance has a token yet: one button connects the
    /// instance this window reads, and a token is the second choice.
    fn render_gitlab_welcome(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let gitlab = &self.menu.gitlab;
        let lead = gitlab.welcome_lead();
        let status = gitlab.hosts.get(lead);
        let host = status.map_or("gitlab.com", |status| status.host.as_str());
        let read_by = status
            .and_then(|status| status.checkout.as_deref())
            .map(|checkout| format!(" for {checkout}"))
            .unwrap_or_default();
        let others: Vec<&str> = gitlab
            .hosts
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != lead)
            .map(|(_, status)| status.host.as_str())
            .collect();
        let connect = status.filter(|status| status.can_connect).map(|_| {
            self.gitlab_connect_button(
                format!("gitlab-connect-{lead}"),
                format!("Connect {host}"),
                host,
                false,
                cx,
            )
        });
        let connecting = gitlab
            .connecting
            .as_ref()
            .filter(|connecting| connecting.host == host);
        div()
            .debug_selector(|| "gitlab-welcome".into())
            .py(px(8.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .child(gitlab_mark(theme.foreground).size(px(44.)))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(self.config.ui.size * 1.2))
                    .child("Connect your GitLab account"),
            )
            .child(
                div()
                    .text_center()
                    .text_color(rgb(theme.subtext()))
                    .child(format!(
                        "The Orchestrator reads issues from {host}{read_by}. Public projects list \
                         without an account; connect for private ones."
                    )),
            )
            .when(!others.is_empty(), |welcome| {
                welcome.child(
                    div()
                        .text_center()
                        .text_color(rgb(theme.muted))
                        .child(format!("Also read: {}", others.join(", "))),
                )
            })
            .children(connect)
            .children(connecting.map(|connecting| self.render_gitlab_connecting(connecting, cx)))
            .child(self.gitlab_link(
                "gitlab-paste",
                if connect_shown(status) {
                    "Use a personal access token instead"
                } else {
                    "Paste a personal access token"
                },
                false,
                cx,
            ))
            .child(
                div()
                    .mt(px(4.))
                    .p(px(10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.background))
                    .text_color(rgb(theme.muted))
                    .child(
                        "Already set up? GITLAB_TOKEN in a project's .envrc or your shell, and \
                         glab, are picked up on their own.",
                    ),
            )
    }

    /// A link that opens the token form; `add` starts it at a new instance.
    fn gitlab_link(
        &self,
        id: &'static str,
        label: &'static str,
        add: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let accent = crate::menu::accent(&self.theme);
        div()
            .id(id)
            .debug_selector(move || id.into())
            .role(Role::Link)
            .aria_label(label)
            .cursor_pointer()
            .text_color(accent)
            .hover(|style| style.underline())
            .when(self.menu.gitlab.is_selected(&Action::Paste), |link| {
                link.underline()
            })
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.show_gitlab_form(add, window, cx);
            }))
    }

    /// The code to approve on GitLab, and the buttons that copy it, open
    /// GitLab, and stop waiting.
    fn render_gitlab_connecting(&self, connecting: &Connecting, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(code) = &connecting.code else {
            return div()
                .debug_selector(|| "gitlab-connecting".into())
                .ml(px(26.))
                .text_color(rgb(theme.muted))
                .child("Asking GitLab for a code\u{2026}");
        };
        div()
            .debug_selector(|| "gitlab-connecting".into())
            .ml(px(26.))
            .p(px(12.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.background))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child("Approve this code on GitLab:")
            .child(
                div().id("gitlab-code-scroll").overflow_x_scroll().child(
                    div()
                        .debug_selector(|| "gitlab-device-code".into())
                        .text_font(&self.config.terminal)
                        .text_size(px(22.))
                        .line_height(px(30.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(crate::sidebar::label_text(code)),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        self.gitlab_button("gitlab-open", "Open GitLab", true, true)
                            .when(self.menu.gitlab.is_selected(&Action::Open), |button| {
                                self.selected_look(button)
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.open_gitlab_approval(cx);
                            })),
                    )
                    .child(
                        self.gitlab_button(
                            "gitlab-copy",
                            if connecting.copied {
                                "Copied"
                            } else {
                                "Copy code"
                            },
                            false,
                            true,
                        )
                        .when(self.menu.gitlab.is_selected(&Action::Copy), |button| {
                            self.selected_look(button)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.copy_gitlab_code(cx);
                        })),
                    )
                    .child(
                        self.gitlab_button("gitlab-cancel", "Cancel", false, true)
                            .when(self.menu.gitlab.is_selected(&Action::Cancel), |button| {
                                self.selected_look(button)
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.cancel_gitlab_connect(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("This page updates once GitLab has the approval."),
            )
    }

    pub(in crate::menu) fn render_gitlab_page(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let gitlab = &self.menu.gitlab;
        let ready = !gitlab.working;
        // Nothing has a token, and some instance could take one.
        let welcome = gitlab.shows_welcome();
        let field =
            |label: &'static str, selector: &'static str, input: &Option<Entity<SearchInput>>| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(label)
                    .children(input.as_ref().map(|input| {
                        div()
                            .debug_selector(move || selector.into())
                            .child(input.clone())
                    }))
            };
        let form = gitlab.form.then(|| {
            div()
                .debug_selector(|| "gitlab-token".into())
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(div().text_color(rgb(theme.muted)).child(
                    "For an instance without Connect. The token needs the read_api scope; \
                     GitLab is asked whom it signs in as before it is saved, only ever over \
                     HTTPS. GITLAB_TOKEN in the project's .envrc or your shell is used before it.",
                ))
                .child(field("Instance", "gitlab-host-input", &gitlab.host))
                .child(field(
                    "Personal access token",
                    "gitlab-token-input",
                    &gitlab.token,
                ))
        });
        let body = div()
            .id("gitlab-body")
            .debug_selector(|| "gitlab-body".into())
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(gitlab.problem.clone().map(|problem| {
                div()
                    .debug_selector(|| "gitlab-problem".into())
                    .text_color(rgb(theme.ink(theme.palette[1])))
                    .child(problem)
            }))
            .when(welcome, |body| body.child(self.render_gitlab_welcome(cx)))
            .when(!welcome && !gitlab.form, |body| {
                body.children(
                    gitlab
                        .hosts
                        .iter()
                        .enumerate()
                        .map(|(index, status)| self.render_gitlab_host(index, status, cx)),
                )
                .child(self.gitlab_link(
                    "gitlab-paste",
                    "Paste a token for an instance\u{2026}",
                    false,
                    cx,
                ))
            })
            .children(form)
            .children(gitlab.message.clone().map(|message| {
                div()
                    .debug_selector(|| "gitlab-message".into())
                    .p(px(10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.background))
                    .child(message)
            }));
        let footer = div()
            .flex_none()
            .p(px(12.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .justify_end()
            .gap(px(8.));
        let footer = if gitlab.form {
            Some(
                footer
                    .child(
                        self.gitlab_button("gitlab-form-cancel", "Cancel", false, true)
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.hide_gitlab_form(window, cx);
                            })),
                    )
                    .child(
                        self.gitlab_button("gitlab-save", "Check and save", true, ready)
                            .when(ready, |button| {
                                button.on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.save_gitlab(cx);
                                }))
                            }),
                    ),
            )
        } else if welcome {
            None
        } else {
            Some(
                footer.child(
                    self.gitlab_button("gitlab-add", "+ Add instance", false, true)
                        .when(gitlab.is_selected(&Action::Add), |button| {
                            self.selected_look(button)
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.show_gitlab_form(true, window, cx);
                        })),
                ),
            )
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .child(self.render_account_tabs(Page::GitLab, cx))
            .child(body)
            .children(footer)
    }
}

/// Where a card's token comes from, in a word or two.
fn source_chip(source: Source, status: &HostStatus) -> &'static str {
    match source {
        Source::Saved if status.connected => "connected",
        Source::Saved => "saved token",
        Source::Envrc => ".envrc",
        Source::Environment => "GITLAB_TOKEN",
        Source::Cli => "glab",
    }
}

fn connect_shown(status: Option<&HostStatus>) -> bool {
    status.is_some_and(|status| status.can_connect)
}

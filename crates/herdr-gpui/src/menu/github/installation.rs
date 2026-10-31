//! What a GitHub App sign-in still needs from the user: installing the App,
//! or approving the permissions it now asks for. The account's GitHub page
//! names each owner with a link to approve it, the title bar's avatar carries
//! a dot until nothing is left, and a toast says so once per window.

use crate::i18n::fl;
use crate::{
    HerdrWindow,
    github::{Access, Installation, Profile},
};
use gpui::{prelude::*, *};

/// Owners the GitHub page lists, each with its own link; the rest are counted.
const LISTED: usize = 5;

/// What the GitHub page and the toast say about one account.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Attention {
    pub(super) title: String,
    pub(super) body: String,
    /// Each owner's approval page, or the App's install page.
    pub(super) links: Vec<(String, String)>,
    pub(super) unlisted: usize,
}

/// `access` as the user reads it, `None` when nothing is needed. `install`
/// is the App's install page, known only for Herdr GPUI's own App.
pub(super) fn attention(login: &str, access: &Access, install: Option<&str>) -> Option<Attention> {
    match access {
        Access::Unknown | Access::Ready => None,
        Access::NotInstalled => Some(Attention {
            title: fl!("github-menu-install-title"),
            body: match install {
                Some(_) => fl!("github-menu-install-body", login = login),
                None => fl!("github-menu-install-custom-body", login = login),
            },
            links: install
                .map(|url| (fl!("github-menu-install-link"), url.to_owned()))
                .into_iter()
                .collect(),
            unlisted: 0,
        }),
        Access::Pending(pending) => Some(Attention {
            title: fl!("github-menu-approve-title"),
            body: fl!("github-menu-approve-body", count = pending.len()),
            links: pending
                .iter()
                .take(LISTED)
                .map(|installation| (owner_label(installation), installation.url()))
                .collect(),
            unlisted: pending.len().saturating_sub(LISTED),
        }),
    }
}

/// Who approves an installation: the user for their own account, an owner
/// for an organization, whose settings page GitHub shows owners only.
fn owner_label(installation: &Installation) -> String {
    if !installation.organization {
        fl!(
            "github-menu-approve-own",
            owner = installation.owner.as_str()
        )
    } else {
        fl!(
            "github-menu-approve-organization",
            owner = installation.owner.as_str()
        )
    }
}

impl HerdrWindow {
    /// Every signed-in account: the device's own, saved SSH devices', and
    /// additional ones.
    fn github_profiles(&self) -> impl Iterator<Item = &Profile> {
        std::iter::once(&self.menu.github)
            .chain(self.menu.github_hosts.values())
            .chain(
                self.menu
                    .github_accounts
                    .iter()
                    .map(|account| &account.auth),
            )
            .filter_map(|auth| auth.profile.as_ref())
    }

    /// Whether any account waits on the user, for the title bar's dot.
    pub(crate) fn github_needs_attention(&self) -> bool {
        self.github_profiles()
            .any(|profile| profile.access.needs_attention())
    }

    /// What the account the GitHub page shows still needs.
    pub(super) fn github_attention(&self) -> Option<Attention> {
        let profile = self.github_auth().profile.as_ref()?;
        attention(
            &profile.login,
            &profile.access,
            self.config.github.app_install_url(),
        )
    }

    /// Open the first link the page offers, for its keyboard action.
    pub(super) fn open_github_attention(&mut self, cx: &mut App) {
        if let Some((_, url)) = self
            .github_attention()
            .and_then(|attention| attention.links.into_iter().next())
        {
            self.open_github_approval(&url, cx);
        }
    }

    fn open_github_approval(&mut self, url: &str, cx: &mut App) {
        self.github_auth_mut()
            .recheck_access(std::time::Instant::now());
        cx.open_url(url);
    }

    /// Toast an account the first time this window finds it waiting, so a
    /// user who never opens the GitHub page still learns why private
    /// repositories fail. Accounts are told apart by login.
    pub(crate) fn toast_github_attention(&mut self, cx: &mut Context<Self>) {
        let install = self.config.github.app_install_url();
        let waiting: Vec<(String, Attention)> = self
            .github_profiles()
            .filter_map(|profile| {
                let attention = attention(&profile.login, &profile.access, install)?;
                Some((profile.login.clone(), attention))
            })
            .collect();
        // Several account slots can hold one login; only the first is told.
        for (login, attention) in waiting {
            if !self.menu.github_toasted.insert(login) {
                continue;
            }
            self.local_transfer_notice(
                &attention.title,
                fl!("github-menu-attention-toast", body = attention.body),
                cx,
            );
        }
    }

    pub(super) fn render_github_attention(&self, cx: &mut Context<Self>) -> Option<Div> {
        let attention = self.github_attention()?;
        let theme = &self.theme;
        let warning = rgb(theme.ink(theme.palette[1]));
        let mut note = div()
            .debug_selector(|| "github-attention".into())
            .mt(px(16.))
            .p(px(10.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(warning)
            .bg(rgb(theme.background))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(warning)
                    .child(attention.title),
            )
            .child(div().child(attention.body));
        // The footer's button opens the first; several owners each get one.
        if attention.links.len() > 1 {
            for (index, (label, url)) in attention.links.into_iter().enumerate() {
                note = note.child(
                    div().flex().child(
                        div()
                            .id(("github-attention-link", index))
                            .debug_selector(move || format!("github-attention-link-{index}"))
                            .px(px(12.))
                            .py(px(7.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .border_1()
                            .border_color(rgb(theme.active))
                            .cursor_pointer()
                            .hover(|style| style.border_color(super::super::accent(theme)))
                            .child(crate::sidebar::label_text(&label))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.open_github_approval(&url, cx);
                            })),
                    ),
                );
            }
        }
        if attention.unlisted > 0 {
            note = note.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("github-menu-more-owners", count = attention.unlisted)),
            );
        }
        Some(
            note.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("github-menu-recheck-note")),
            ),
        )
    }
}

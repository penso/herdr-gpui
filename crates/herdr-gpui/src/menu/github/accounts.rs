//! Additional GitHub accounts on the GitHub page: one sign-in per
//! `[github] accounts` entry, the strip that switches between them and the
//! device's own account, adding and removing them, and signing in with a
//! pasted token. Which account reads a repository is `github::Tokens`' rule.

use crate::i18n::fl;
use crate::{
    HerdrWindow,
    config::{Config, GitHubAccount},
    github::{Account, Auth, Profile, Tokens},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

/// An additional account's sign-in and the owners its config entry lists.
pub(crate) struct AdditionalAccount {
    pub(crate) id: String,
    /// Owners read besides the account's own login and organizations.
    pub(crate) owners: Vec<String>,
    pub(crate) auth: Auth,
}

impl AdditionalAccount {
    /// Every owner the account reads, as far as it is known yet: its login
    /// and organizations once signed in, then the owners its config lists.
    pub(crate) fn covered(&self) -> Vec<&str> {
        let mut covered: Vec<&str> = Vec::new();
        let profile = self.auth.profile.as_ref();
        let names = profile
            .map(|profile| &profile.login)
            .into_iter()
            .chain(
                profile
                    .into_iter()
                    .flat_map(|profile| profile.organizations.iter().flatten()),
            )
            .chain(&self.owners);
        for name in names {
            if !covered.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
                covered.push(name);
            }
        }
        covered
    }
}

impl HerdrWindow {
    /// Keep one sign-in per configured account, in config order. An account
    /// gone from the config is dropped from memory; removing it from the
    /// GitHub page deletes its saved credential first.
    pub(crate) fn sync_github_accounts(&mut self) {
        let configured = &self.config.github.accounts;
        let accounts = &mut self.menu.github_accounts;
        accounts.retain(|account| configured.iter().any(|entry| entry.id == account.id));
        for entry in configured {
            match accounts.iter_mut().find(|account| account.id == entry.id) {
                Some(account) => {
                    if account.owners != entry.owners {
                        account.owners.clone_from(&entry.owners);
                    }
                }
                // Headless windows never touch saved credentials.
                None if self.avatars.is_some() => {
                    let Some(kind) = Account::additional(&entry.id) else {
                        continue;
                    };
                    let mut auth = Auth::for_account(kind);
                    auth.initialize(&self.config.github);
                    accounts.push(AdditionalAccount {
                        id: entry.id.clone(),
                        owners: entry.owners.clone(),
                        auth,
                    });
                }
                None => {}
            }
        }
        if accounts
            .windows(2)
            .any(|pair| position(configured, &pair[0].id) > position(configured, &pair[1].id))
        {
            accounts.sort_by_key(|account| position(configured, &account.id));
        }
        // An account just added is shown once its sign-in exists.
        if let Some(id) = &self.menu.github_adding
            && accounts.iter().any(|account| &account.id == id)
        {
            self.menu.github_account = self.menu.github_adding.take();
        }
        if self
            .menu
            .github_account
            .as_ref()
            .is_some_and(|id| !accounts.iter().any(|account| &account.id == id))
        {
            self.menu.github_account = None;
        }
    }

    /// The additional account the GitHub page shows.
    pub(super) fn selected_github_account(&self) -> Option<&AdditionalAccount> {
        let id = self.menu.github_account.as_ref()?;
        self.menu
            .github_accounts
            .iter()
            .find(|account| &account.id == id)
    }

    /// The tokens pull requests on the selected device are read with: its
    /// account (see `pr_profile`) and every signed-in additional account.
    pub(crate) fn pr_tokens(&self) -> Option<Tokens> {
        self.pr_profile().map(|main| self.tokens_over(main))
    }

    /// The same with this computer's main account, for work that reads only
    /// local checkouts.
    pub(crate) fn main_tokens(&self) -> Option<Tokens> {
        self.menu
            .github
            .profile
            .as_ref()
            .map(|main| self.tokens_over(main))
    }

    pub(crate) fn tokens_over(&self, main: &Profile) -> Tokens {
        Tokens::new(
            main,
            self.menu.github_accounts.iter().filter_map(|account| {
                Some((account.auth.profile.as_ref()?, account.owners.as_slice()))
            }),
        )
    }

    /// Whether the page offers to add an account: once the device's own is
    /// signed in, or another additional one exists, and below the limit.
    pub(super) fn can_add_github_account(&self) -> bool {
        let device = self
            .github_host()
            .and_then(|id| self.menu.github_hosts.get(id))
            .unwrap_or(&self.menu.github);
        self.menu.github_accounts.len() < crate::config::MAX_ACCOUNTS
            && (device.connected() || !self.menu.github_accounts.is_empty())
    }

    /// Add an account and show it, so it can be signed in. The config edit
    /// runs off the UI thread; the reload that follows creates its sign-in.
    pub(super) fn add_github_account(&mut self, cx: &mut Context<Self>) {
        if self.config.github.accounts.len() >= crate::config::MAX_ACCOUNTS {
            self.menu.error = Some(fl!(
                "github-menu-too-many-accounts",
                count = crate::config::MAX_ACCOUNTS
            ));
            return;
        }
        let account = match GitHubAccount::generate() {
            Ok(account) => account,
            Err(error) => {
                self.menu.error = Some(error.to_string());
                return;
            }
        };
        tracing::info!(
            category = "github_accounts",
            "Adding an additional GitHub account"
        );
        self.close_github_token();
        self.menu.error = None;
        self.menu.github_adding = Some(account.id.clone());
        self.write_preference(move || Config::add_github_account(&account), cx);
    }

    /// Delete the shown additional account's saved credential, then its
    /// config entry. Its repositories are read with the main account again.
    pub(super) fn remove_github_account(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.selected_github_account() else {
            return;
        };
        let id = account.id.clone();
        let store = account.auth.store();
        let Some(kind) = Account::additional(&id) else {
            return;
        };
        tracing::info!(
            category = "github_accounts",
            "Removing an additional GitHub account"
        );
        // Signed out at once, so nothing reads with it while the removal
        // runs; the credential is deleted below rather than by this sign-in.
        self.menu.github_accounts.retain(|account| account.id != id);
        self.menu.github_account = None;
        self.menu.pr_cache.clear();
        self.menu.pr.clear();
        self.write_preference(
            move || {
                crate::github::forget(store, &kind)?;
                Config::remove_github_account(&id)
            },
            cx,
        );
    }

    /// Show the pasted-token field for the account the page shows.
    pub(super) fn open_github_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_masked(true, cx);
            input.set_placeholder_with(|| fl!("github-menu-token-placeholder"), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        self.menu.github_token = Some(input);
    }

    pub(super) fn close_github_token(&mut self) {
        self.menu.github_token = None;
    }

    /// Check and save the pasted token for the account the page shows.
    pub(super) fn submit_github_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.menu.github_token.as_ref() else {
            return;
        };
        let text = input.read(cx).text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let token = SecretString::from(text);
        self.close_github_token();
        window.focus(&self.menu.focus, cx);
        let config = &self.config;
        match self
            .selected_github_account_index()
            .map(|index| &mut self.menu.github_accounts[index].auth)
        {
            Some(auth) => auth.use_token(token, &config.github),
            None => {
                let host = self.github_host().map(str::to_owned);
                let menu = &mut self.menu;
                host.and_then(|id| menu.github_hosts.get_mut(&id))
                    .unwrap_or(&mut menu.github)
                    .use_token(token, &config.github);
            }
        }
    }

    pub(super) fn selected_github_account_index(&self) -> Option<usize> {
        let id = self.menu.github_account.as_ref()?;
        self.menu
            .github_accounts
            .iter()
            .position(|account| &account.id == id)
    }

    /// Show the account before (`forward` false) or after the one shown,
    /// the device's own account first.
    pub(super) fn cycle_github_account(&mut self, forward: bool) {
        let count = self.menu.github_accounts.len() + 1;
        if count == 1 {
            return;
        }
        let current = self
            .selected_github_account_index()
            .map_or(0, |index| index + 1);
        let next = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        self.select_github_account(
            next.checked_sub(1)
                .map(|index| self.menu.github_accounts[index].id.clone()),
        );
    }

    pub(super) fn select_github_account(&mut self, id: Option<String>) {
        if self.menu.github_account == id {
            return;
        }
        self.menu.github_account = id;
        self.menu.github_selected = None;
        self.menu.error = None;
        self.close_github_token();
        self.github_auth_mut()
            .recheck_access(std::time::Instant::now());
    }

    /// The strip naming the device's account and each additional one, with
    /// Add account at its end, once there is an additional account to show.
    pub(super) fn render_github_accounts(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.menu.github_accounts.is_empty() {
            return None;
        }
        let device = self
            .github_host()
            .and_then(|id| self.menu.github_hosts.get(id))
            .unwrap_or(&self.menu.github);
        let theme = &self.theme;
        let chip = |id: SharedString, label: String, selected: bool| {
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .flex_none()
                .max_w(px(180.))
                .truncate()
                .px(px(8.))
                .py(px(3.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .border_color(rgb(if selected {
                    theme.foreground
                } else {
                    theme.active
                }))
                .when(selected, |chip| chip.bg(rgb(theme.active)))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.active)))
                .child(crate::sidebar::label_text(&label))
        };
        let login = |auth: &Auth, fallback: String| {
            auth.profile
                .as_ref()
                .map_or_else(|| fallback, |profile| format!("@{}", profile.login))
        };
        let mut strip = div()
            .debug_selector(|| "github-accounts".into())
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .mb(px(12.))
            .child(
                chip(
                    "github-account-main".into(),
                    login(device, fl!("github-menu-main-account")),
                    self.menu.github_account.is_none(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.select_github_account(None);
                    cx.notify();
                })),
            );
        for (index, account) in self.menu.github_accounts.iter().enumerate() {
            let id = account.id.clone();
            let selected = self.menu.github_account.as_ref() == Some(&account.id);
            strip = strip.child(
                chip(
                    format!("github-account-{index}").into(),
                    login(&account.auth, fl!("github-menu-new-account")),
                    selected,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.select_github_account(Some(id.clone()));
                    cx.notify();
                })),
            );
        }
        if self.can_add_github_account() {
            strip = strip.child(
                chip(
                    "github-account-add".into(),
                    fl!("github-menu-add-account-chip"),
                    false,
                )
                .text_color(rgb(theme.muted))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.add_github_account(cx);
                    cx.notify();
                })),
            );
        }
        Some(strip)
    }

    /// What the shown additional account is for, and which owners it reads.
    pub(super) fn github_account_note(&self) -> Option<String> {
        let account = self.selected_github_account()?;
        let covered = account.covered();
        Some(if covered.is_empty() {
            fl!("github-menu-account-note")
        } else {
            fl!("github-menu-account-owners", owners = covered.join(", "))
        })
    }

    /// The pasted-token field and how to make a token, while it is open.
    pub(super) fn render_github_token(&self) -> Option<Div> {
        let input = self.menu.github_token.clone()?;
        Some(
            div()
                .debug_selector(|| "github-token".into())
                .mt(px(12.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(fl!("github-menu-token-label"))
                .child(input)
                .child(
                    div()
                        .text_color(rgb(self.theme.muted))
                        .child(fl!("github-menu-token-help")),
                ),
        )
    }
}

fn position(configured: &[GitHubAccount], id: &str) -> usize {
    configured
        .iter()
        .position(|entry| entry.id == id)
        .unwrap_or(usize::MAX)
}

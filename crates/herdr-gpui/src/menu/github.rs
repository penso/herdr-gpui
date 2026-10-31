// i18n: migrated
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    fonts::StyledFont,
    github::{Account, Auth, Profile},
};
use gpui::{prelude::*, *};
use herdr_client::ConnectTarget;

mod accounts;
mod installation;
pub(crate) use accounts::AdditionalAccount;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Copy,
    Open,
    GitHubCli,
    Start,
    /// Show the field for a pasted personal access token.
    Token,
    /// Add an account read for the repositories it owns.
    Add,
    SubmitToken,
    CancelToken,
    /// Open GitHub where the App is installed or its permissions approved.
    Approve,
    SignOut,
    Close,
}

impl Action {
    fn id(self) -> &'static str {
        match self {
            Self::Copy => "github-copy",
            Self::Open => "github-open",
            Self::GitHubCli => "github-cli",
            Self::Start => "github-start",
            Self::Token => "github-token-open",
            Self::Add => "github-account-add-action",
            Self::SubmitToken => "github-token-submit",
            Self::CancelToken => "github-token-cancel",
            Self::Approve => "github-approve",
            Self::SignOut => "github-sign-out",
            Self::Close => "github-header-close",
        }
    }
}

impl HerdrWindow {
    /// The saved SSH device whose own account the GitHub page manages, when
    /// that device is selected. Local and explicit sockets use the main account.
    fn github_host(&self) -> Option<&str> {
        self.endpoint_github_host(self.selected_endpoint)
    }

    /// [`Self::github_host`] for any device, selected or not.
    fn endpoint_github_host(&self, index: usize) -> Option<&str> {
        let endpoint = self.endpoints.get(index)?;
        matches!(endpoint.connection.target, ConnectTarget::Ssh { .. })
            .then_some(endpoint.id.as_str())
            .filter(|id| self.menu.github_hosts.contains_key(*id))
    }

    /// The account the GitHub page shows and signs in or out: an additional
    /// account when one is chosen, else the device's own.
    pub(crate) fn github_auth(&self) -> &Auth {
        if let Some(account) = self.selected_github_account() {
            return &account.auth;
        }
        self.github_host()
            .and_then(|id| self.menu.github_hosts.get(id))
            .unwrap_or(&self.menu.github)
    }

    fn github_auth_mut(&mut self) -> &mut Auth {
        if let Some(index) = self.selected_github_account_index() {
            return &mut self.menu.github_accounts[index].auth;
        }
        match self.github_host().map(str::to_owned) {
            Some(id) => self
                .menu
                .github_hosts
                .get_mut(&id)
                .unwrap_or(&mut self.menu.github),
            None => &mut self.menu.github,
        }
    }

    /// Whether this window reads and writes saved sign-ins. Headless windows
    /// never touch them, so tests leave the user's keyring alone.
    pub(crate) fn reads_credentials(&self) -> bool {
        self.avatars.is_some()
    }

    /// Start signing in the account the GitHub page shows.
    pub(crate) fn start_github(&mut self) {
        let config = &self.config;
        if let Some(index) = self.selected_github_account_index() {
            self.menu.github_accounts[index].auth.start(&config.github);
            return;
        }
        let host = self.github_host().map(str::to_owned);
        let menu = &mut self.menu;
        host.and_then(|id| menu.github_hosts.get_mut(&id))
            .unwrap_or(&mut menu.github)
            .start(&config.github);
    }

    /// The account pull requests on the selected device are read with: the
    /// device's own sign-in when it has one, otherwise the main account.
    pub(crate) fn pr_profile(&self) -> Option<&Profile> {
        self.endpoint_pr_profile(self.selected_endpoint)
    }

    /// [`Self::pr_profile`] for any device, selected or not.
    pub(crate) fn endpoint_pr_profile(&self, index: usize) -> Option<&Profile> {
        self.endpoint_github_host(index)
            .and_then(|id| self.menu.github_hosts.get(id)?.profile.as_ref())
            .or(self.menu.github.profile.as_ref())
    }

    /// Keep one account per saved SSH device. A removed device's account is
    /// dropped from memory; its saved credential stays until signed out, so
    /// re-adding the device finds it again.
    pub(crate) fn sync_github_hosts(&mut self) {
        let hosts = &mut self.menu.github_hosts;
        hosts.retain(|id, _| {
            self.endpoints.iter().any(|endpoint| {
                &endpoint.id == id
                    && matches!(endpoint.connection.target, ConnectTarget::Ssh { .. })
            })
        });
        // Headless windows never touch saved credentials, like the main account.
        if self.avatars.is_none() {
            return;
        }
        for endpoint in &self.endpoints {
            if !matches!(endpoint.connection.target, ConnectTarget::Ssh { .. })
                || hosts.contains_key(&endpoint.id)
            {
                continue;
            }
            let Some(account) =
                crate::endpoint::saved_profile_id(&endpoint.id).and_then(Account::host)
            else {
                continue;
            };
            let mut auth = Auth::for_account(account);
            auth.initialize(&self.config.github);
            hosts.insert(endpoint.id.clone(), auth);
        }
    }

    /// Start the GitHub CLI's sign-in for the main account.
    fn use_github_cli(&mut self) {
        self.menu.github.use_github_cli(&self.config.github);
    }

    pub(crate) fn poll_github(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_github_hosts();
        self.sync_github_accounts();
        self.prune_device_removals();
        // Only the main account's page offers the local GitHub CLI, and only
        // a window that may touch credentials asks it.
        if self.avatars.is_some()
            && self.menu.page == Some(super::Page::GitHub)
            && self.github_host().is_none()
            && self.menu.github_account.is_none()
        {
            self.menu.github.probe_github_cli(std::time::Instant::now());
        }
        if self.menu.page == Some(super::Page::GitHub) {
            let now = std::time::Instant::now();
            let host = self.github_host().map(str::to_owned);
            let menu = &mut self.menu;
            host.and_then(|id| menu.github_hosts.get_mut(&id))
                .unwrap_or(&mut menu.github)
                .recheck_access(now);
            for account in &mut menu.github_accounts {
                account.auth.recheck_access(now);
            }
        }
        let connected = self.github_auth().connected();
        let mut changed = self.menu.github.poll();
        for auth in self.menu.github_hosts.values_mut() {
            changed |= auth.poll();
        }
        for account in &mut self.menu.github_accounts {
            changed |= account.auth.poll();
        }
        let menu = &mut self.menu;
        let avatars = std::iter::once(&menu.github)
            .chain(menu.github_hosts.values())
            .chain(menu.github_accounts.iter().map(|account| &account.auth))
            .filter_map(|auth| auth.profile.as_ref())
            .map(|profile| profile.avatar_url.as_str());
        changed |= menu.github_avatars.poll(avatars);
        if changed {
            // An additional account stays shown, with the owners it reads.
            if !connected
                && self.github_auth().connected()
                && self.menu.page == Some(super::Page::GitHub)
                && self.menu.github_account.is_none()
            {
                self.dismiss_menu(window, cx);
            }
            self.toast_github_attention(cx);
            cx.notify();
        }
    }

    fn github_actions(&self) -> Vec<Action> {
        let auth = self.github_auth();
        let mut actions = Vec::new();
        if self.menu.github_token.is_some() {
            return vec![Action::SubmitToken, Action::CancelToken, Action::Close];
        }
        if auth.code().is_some() {
            actions.extend([Action::Copy, Action::Open]);
        } else if !auth.busy() && !auth.loading_profile() && !auth.connected() {
            if self.github_host().is_none() && auth.github_cli_available() {
                actions.push(Action::GitHubCli);
            }
            actions.extend([Action::Start, Action::Token]);
        }
        if auth.connected() && self.github_attention().is_some_and(|a| !a.links.is_empty()) {
            actions.push(Action::Approve);
        }
        // An additional account can be removed even before it signs in.
        if (auth.can_sign_out() || self.menu.github_account.is_some())
            && !auth.busy()
            && !auth.loading_profile()
        {
            actions.push(Action::SignOut);
        }
        // Once the device's account is in, another can be added beside it.
        if self.menu.github_account.is_none()
            && auth.connected()
            && self.menu.github_accounts.len() < crate::config::MAX_ACCOUNTS
        {
            actions.push(Action::Add);
        }
        actions.push(Action::Close);
        actions
    }

    fn github_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if !self.github_actions().contains(&action) {
            return;
        }
        match action {
            Action::Copy => {
                if let Some(code) = self.github_auth_mut().copy_code() {
                    cx.write_to_clipboard(ClipboardItem::new_string(code.to_owned()));
                }
            }
            Action::Open => cx.open_url(crate::github::VERIFY_URL),
            Action::GitHubCli => self.use_github_cli(),
            Action::Start => self.start_github(),
            Action::Token => self.open_github_token(window, cx),
            Action::Add => self.add_github_account(cx),
            Action::Approve => self.open_github_attention(cx),
            Action::SubmitToken => self.submit_github_token(window, cx),
            Action::CancelToken => {
                self.close_github_token();
                window.focus(&self.menu.focus, cx);
            }
            Action::SignOut if self.menu.github_account.is_some() => {
                self.remove_github_account(cx);
            }
            Action::SignOut => {
                self.menu.pr_cache.clear();
                self.menu.pr.clear();
                self.github_auth_mut().sign_out();
            }
            Action::Close => self.dismiss_menu(window, cx),
        }
        cx.notify();
    }

    pub(super) fn github_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        // The token field edits itself; only Escape and Enter are the page's.
        if let Some(input) = &self.menu.github_token
            && (input.read(cx).is_composing() || !matches!(key, "escape" | "enter"))
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        let actions = self.github_actions();
        if key == "tab" && !modifiers.control && !modifiers.platform && !modifiers.alt {
            let selected = self
                .menu
                .github_selected
                .and_then(|action| actions.iter().position(|candidate| *candidate == action));
            self.menu.github_selected = Some(
                actions[match selected {
                    None if modifiers.shift => actions.len() - 1,
                    None => 0,
                    Some(index) => {
                        (index
                            + if modifiers.shift {
                                actions.len() - 1
                            } else {
                                1
                            })
                            % actions.len()
                    }
                }],
            );
            if self.menu.github_selected == Some(Action::Copy) {
                self.menu.github_scroll.set_offset(Point::default());
            }
        } else if key == "c" && crate::actions::edit_modifiers(modifiers) {
            self.github_action(Action::Copy, window, cx);
        } else if modifiers == Modifiers::default() {
            let action = match key {
                "escape" if self.menu.github_token.is_some() => Some(Action::CancelToken),
                "enter" if self.menu.github_token.is_some() => Some(Action::SubmitToken),
                "escape" => Some(Action::Close),
                "a" => {
                    if self.can_add_github_account() {
                        self.add_github_account(cx);
                    }
                    None
                }
                "left" | "right" => {
                    self.cycle_github_account(key == "right");
                    None
                }
                "t" => Some(Action::Token),
                "r" => Some(Action::Approve),
                "c" => {
                    if self.github_auth().busy() {
                        self.github_auth_mut().cancel();
                    }
                    None
                }
                "o" => Some(Action::Open),
                "s" => Some(Action::Start),
                "g" => Some(Action::GitHubCli),
                "d" => Some(Action::SignOut),
                "enter" | "space" => self.menu.github_selected,
                "up" | "down" | "pageup" | "pagedown" => {
                    let scroll = &self.menu.github_scroll;
                    let distance = if key.starts_with("page") {
                        scroll.bounds().size.height * 0.8
                    } else {
                        px(self.config.ui.line_height() * 3.)
                    };
                    scroll.set_offset(
                        scroll.offset()
                            + point(
                                px(0.),
                                distance * if key.ends_with("up") { 1. } else { -1. },
                            ),
                    );
                    None
                }
                _ => None,
            };
            if let Some(action) = action {
                self.github_action(action, window, cx);
            }
        }
        cx.notify();
    }

    fn github_button(&self, action: Action, label: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        // The GitHub CLI's sign-in, when offered, is the one-click choice.
        let primary = match action {
            Action::Open | Action::GitHubCli | Action::SubmitToken | Action::Approve => true,
            Action::Start => !self.github_actions().contains(&Action::GitHubCli),
            Action::Copy
            | Action::Token
            | Action::Add
            | Action::CancelToken
            | Action::SignOut
            | Action::Close => false,
        };
        let danger = action == Action::SignOut;
        let accent = if danger {
            super::danger(theme)
        } else {
            super::accent(theme)
        };
        let selected = self.menu.github_selected == Some(action);
        div()
            .id(action.id())
            .debug_selector(move || action.id().into())
            .flex_none()
            .px(px(12.))
            .py(px(7.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(if selected {
                if danger {
                    accent
                } else {
                    rgb(theme.foreground)
                }
            } else {
                rgb(theme.active)
            })
            .bg(if primary {
                accent
            } else {
                rgb(theme.background)
            })
            .text_color(if primary {
                rgb(theme.background)
            } else if danger {
                accent
            } else {
                rgb(theme.foreground)
            })
            .font_weight(if primary {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .cursor_pointer()
            .hover(|style| style.border_color(accent))
            .child(crate::sidebar::label_text(label))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.github_action(action, window, cx);
            }))
    }

    pub(super) fn render_github_auth(&self, cx: &mut Context<Self>) -> Div {
        let auth = self.github_auth();
        let theme = &self.theme;
        let mut body = div()
            .id("github-body")
            .debug_selector(|| "github-body".into())
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.menu.github_scroll)
            .p(px(16.));
        let additional = self.menu.github_account.is_some();
        let host = self
            .github_host()
            .filter(|_| !additional)
            .map(|_| self.endpoints[self.selected_endpoint].label.as_str());
        if let Some(strip) = self.render_github_accounts(cx) {
            body = body.child(strip);
        }
        if let Some(note) = self.github_account_note() {
            body = body.child(
                div()
                    .debug_selector(|| "github-account-note".into())
                    .mb(px(12.))
                    .text_color(rgb(theme.subtext()))
                    .child(note),
            );
        }
        if let Some(label) = host {
            let note = match (&auth.profile, &self.menu.github.profile) {
                (Some(_), _) => fl!("github-menu-host-own", device = label),
                (None, Some(main)) => fl!(
                    "github-menu-host-main",
                    device = label,
                    login = main.login.as_str()
                ),
                (None, None) => fl!("github-menu-host-signed-out", device = label),
            };
            body = body.child(
                div()
                    .debug_selector(|| "github-host-note".into())
                    .mb(px(12.))
                    .text_color(rgb(theme.subtext()))
                    .child(note),
            );
        }
        if let Some(profile) = &auth.profile {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(40.))
                            .flex_none()
                            .rounded_full()
                            .overflow_hidden()
                            // Round the image itself: GPUI 0.2.2 clips overflow
                            // to the box, not to its corner radii.
                            .debug_selector(|| "github-avatar".into())
                            .child(match self.menu.github_avatars.get(&profile.avatar_url) {
                                Some(image) => {
                                    img(image).size_full().rounded_full().into_any_element()
                                }
                                None => crate::sidebar::github_mark(theme.muted)
                                    .size_full()
                                    .into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .debug_selector(|| "github-connected".into())
                                    .text_color(rgb(theme.ink(theme.palette[2])))
                                    .child(if profile.source.github_cli() {
                                        fl!("github-menu-connected-cli")
                                    } else {
                                        fl!("github-menu-connected")
                                    }),
                            )
                            .child(
                                div()
                                    .id("github-account")
                                    .overflow_x_scroll()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("@{}", profile.login)),
                            ),
                    ),
            );
        }
        if let Some(code) = auth.code() {
            body = body
                .child(div().mb(px(12.)).child(fl!("github-menu-copy-code-step")))
                .child(
                    div()
                        .p(px(16.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.background))
                        .child(
                            div().id("github-code-scroll").overflow_x_scroll().child(
                                div()
                                    .debug_selector(|| "github-device-code".into())
                                    .text_font(&self.config.terminal)
                                    .text_size(px(24.))
                                    .line_height(px(32.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::sidebar::label_text(code)),
                            ),
                        )
                        .child(div().mt(px(12.)).child(self.github_button(
                            Action::Copy,
                            &if auth.copied() {
                                fl!("github-menu-copied")
                            } else {
                                fl!("github-menu-copy-code")
                            },
                            cx,
                        ))),
                )
                .child(div().mt(px(16.)).child(fl!("github-menu-paste-code-step")))
                .child(
                    div()
                        .mt(px(4.))
                        .text_color(rgb(theme.muted))
                        .child("github.com/login/device"), // i18n: data
                );
        }
        let message = auth.message.clone().unwrap_or_else(|| {
            if auth.loading_profile() {
                fl!("github-menu-checking")
            } else if auth.connected() {
                fl!("github-menu-ready")
            } else if host.is_none() && !additional && auth.github_cli_available() {
                fl!("github-menu-cli-available")
            } else {
                fl!("github-menu-sign-in-note")
            }
        });
        if let Some(field) = self.render_github_token() {
            body = body.child(field);
        } else if !auth.connected() || auth.failed {
            body = body.child(
                div()
                    .debug_selector(|| "github-status".into())
                    .mt(px(16.))
                    .p(px(10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.background))
                    .text_color(rgb(if auth.failed {
                        theme.ink(theme.palette[1])
                    } else {
                        theme.muted
                    }))
                    .child(message),
            );
        }
        if let Some(attention) = self.render_github_attention(cx) {
            body = body.child(attention);
        }
        if let Some(note) = auth
            .profile
            .as_ref()
            .and_then(|profile| github_cli_note(profile, auth.store()))
        {
            body = body.child(
                div()
                    .debug_selector(|| "github-cli-note".into())
                    .mt(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(note),
            );
        }
        if let Some(note) = crate::github::store_note(auth.store(), auth.connected()) {
            let (color, text) = match note {
                crate::github::Note::Warning(text) => (theme.ink(theme.palette[1]), text),
                crate::github::Note::Info(text) => (theme.muted, text),
            };
            body = body.child(div().mt(px(12.)).text_color(rgb(color)).child(text));
        }
        let mut footer = div()
            .debug_selector(|| "github-footer".into())
            .flex_none()
            .p(px(12.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(px(8.));
        let mut has_footer = false;
        for action in self
            .github_actions()
            .into_iter()
            .filter(|action| !matches!(action, Action::Copy | Action::Close))
        {
            has_footer = true;
            if action == Action::Approve {
                let label = self
                    .github_attention()
                    .and_then(|attention| attention.links.into_iter().next())
                    .map_or_else(String::new, |(label, _)| {
                        fl!("github-menu-approve-key", label = label)
                    });
                footer = footer.child(self.github_button(action, &label, cx));
                continue;
            }
            let label = match action {
                Action::Open => fl!("github-menu-open"),
                Action::GitHubCli => fl!("github-menu-use-cli"),
                Action::Start if auth.failed => fl!("github-menu-try-again"),
                Action::Start if host.is_some() && self.menu.github.connected() => {
                    fl!("github-menu-use-another")
                }
                Action::Start if additional => fl!("github-menu-sign-in-browser"),
                Action::Start => fl!("github-menu-sign-in"),
                Action::Token => fl!("github-menu-use-token"),
                Action::Add => fl!("github-menu-add-account"),
                Action::SubmitToken => fl!("github-menu-save-token"),
                Action::CancelToken => fl!("github-menu-cancel-token"),
                Action::SignOut if additional => fl!("github-menu-remove-account"),
                Action::SignOut
                    if auth
                        .profile
                        .as_ref()
                        .is_some_and(|profile| profile.source.github_cli()) =>
                {
                    fl!("github-menu-stop-cli")
                }
                Action::SignOut => fl!("github-menu-sign-out"),
                Action::Copy | Action::Approve | Action::Close => continue,
            };
            footer = footer.child(self.github_button(action, &label, cx));
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .child(self.render_account_tabs(super::Page::GitHub, cx))
            .child(body)
            .when(has_footer, |panel| panel.child(footer))
    }
}

/// What a GitHub CLI session should say: that `gh` owns the token, and how to
/// reach private repositories when its scopes leave them out.
fn github_cli_note(profile: &Profile, store: crate::github::Store) -> Option<String> {
    match profile.source {
        crate::github::Source::GitHubCli { repo_scope: false } => {
            Some(fl!("github-menu-cli-no-repo-scope"))
        }
        crate::github::Source::GitHubCli { .. } if store == crate::github::Store::Environment => {
            Some(fl!("github-menu-cli-session"))
        }
        crate::github::Source::GitHubCli { .. } => Some(fl!("github-menu-cli-keeps-token")),
        crate::github::Source::Token => None,
    }
}

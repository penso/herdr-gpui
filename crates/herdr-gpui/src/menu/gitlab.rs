//! The account menu's GitLab page: each GitLab instance the windows'
//! Orchestrators read, the token it is read with and where that comes from,
//! whom GitLab signs it in as, and a pasted token checked with GitLab before
//! it is saved. Every lookup blocks on direnv, the credential store, `glab`,
//! or the network, so it runs on the background executor and lands back here.

// i18n: migrated
use super::Page;
use crate::i18n::fl;
use crate::{HerdrWindow, orchestrator::GitLabRead, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_orchestrator::gitlab::{
    self, Api, Scheme,
    account::{self, Source},
    cli_hosts::{self, CliHost},
    saved::{self, Kept, Saved},
};
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod connect;
mod keys;
mod render;
#[cfg(test)]
mod tests;

/// gitlab.com, listed when no repository names another instance.
const DEFAULT_HOST: &str = "gitlab.com";
/// What a save or forget says in a window that keeps no saved credential.
pub(super) fn no_store() -> String {
    fl!("gitlab-menu-no-store")
}

/// What is known of an instance's token.
pub(crate) enum Token {
    /// Not looked up yet.
    Unknown,
    None,
    Found {
        /// The token the instance is read with, which may not be the saved one.
        source: Source,
        /// Whom GitLab signs the token in as, once asked.
        user: Option<Result<account::User, Arc<gitlab::Error>>>,
    },
}

/// One instance the page lists.
pub(crate) struct HostStatus {
    pub(crate) host: String,
    /// Every remote reads the instance over plain HTTP, so no token is sent
    /// to it or checked with it.
    pub(crate) plain: bool,
    /// A token is saved for the instance, so Forget is offered even while an
    /// `.envrc` or `GITLAB_TOKEN` takes precedence over it.
    pub(crate) saved: bool,
    /// What is saved is an account connected with OAuth, which Disconnect
    /// removes.
    pub(crate) connected: bool,
    /// An OAuth application connects accounts on the instance.
    pub(crate) can_connect: bool,
    /// The folder name of the checkout whose `.envrc` was asked, if any.
    pub(crate) checkout: Option<String>,
    pub(crate) token: Token,
}

/// An instance the page lists, before it is looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) api: Api,
    /// The first checkout on this computer read from it, whose `.envrc` may
    /// set its token.
    pub(crate) checkout: Option<PathBuf>,
}

/// The page's state, kept while the menu closes so a reopened page shows
/// the last lookup at once.
#[derive(Default)]
pub(crate) struct GitLabMenu {
    pub(crate) hosts: Vec<HostStatus>,
    /// Instances showing their last lookup while a new one runs, so a card
    /// and its picture hold still; their buttons wait for it.
    pub(crate) checking: BTreeSet<String>,
    /// Set to stop the lookups under way, when the next check replaces them
    /// or the menu closes, so a slow `.envrc` is not loaded for nothing.
    stop: Arc<AtomicBool>,
    /// The API each listed instance is read from, so a pasted token is
    /// checked where its projects are read.
    apis: BTreeMap<String, Api>,
    /// This window's instances, which the host field starts at first.
    preferred: Vec<String>,
    pub(super) host: Option<Entity<SearchInput>>,
    pub(super) token: Option<Entity<SearchInput>>,
    /// A save or forget under way; the buttons wait for it.
    working: bool,
    pub(crate) message: Option<String>,
    /// Why the saved tokens could not be read, shown above the instances.
    pub(crate) problem: Option<String>,
    /// The saved-token read under way, replaced by the next one.
    _task: Option<Task<()>>,
    /// Each instance's lookup, landing on its own row as it finishes, so one
    /// slow instance never holds the others. Replaced by the next check.
    _lookups: Vec<Task<()>>,
    /// The save or forget under way. Kept apart from lookups, which reopening
    /// the page starts again, so its result is still shown and acted on.
    _save: Option<Task<()>>,
    /// An account being connected with OAuth.
    pub(crate) connecting: Option<connect::Connecting>,
    /// The token form shows, opened from the page's link or Add instance.
    pub(crate) form: bool,
    /// Each signed-in account's picture.
    avatars: crate::avatars::ProfileAvatars,
    /// The control the keyboard selected, which Enter or Space uses.
    pub(crate) selected: Option<keys::Action>,
}

impl GitLabMenu {
    /// Stops the lookups under way; their rows keep what they show.
    pub(crate) fn stop_lookups(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stop = Arc::default();
        self._task = None;
        self._lookups.clear();
        self.checking.clear();
    }

    /// The instance the welcome page offers to connect: one this window
    /// reads first, then any other that can connect, else the first listed.
    pub(crate) fn welcome_lead(&self) -> usize {
        let connectable = |(_, status): &(usize, &HostStatus)| status.can_connect;
        let mut hosts = self.hosts.iter().enumerate();
        hosts
            .clone()
            .filter(connectable)
            .find(|(_, status)| self.preferred.contains(&status.host))
            .or_else(|| hosts.find(connectable))
            .map_or(0, |(index, _)| index)
    }
}

/// Where a token came from, as the page says it.
fn source_label(source: Source, status: &HostStatus) -> String {
    match (source, status.checkout.as_deref()) {
        (Source::Envrc, Some(checkout)) => fl!("gitlab-menu-from-envrc-of", checkout = checkout),
        (Source::Envrc, None) => fl!("gitlab-menu-from-envrc"),
        (Source::Environment, _) => fl!("gitlab-menu-from-environment"),
        (Source::Saved, _) if status.connected => fl!("gitlab-menu-from-connected"),
        (Source::Saved, _) => fl!("gitlab-menu-from-saved"),
        (Source::Cli, _) => fl!("gitlab-menu-from-cli"),
    }
}

/// What an instance's row says.
pub(crate) fn status_line(status: &HostStatus) -> String {
    if status.plain {
        return fl!("gitlab-menu-plain");
    }
    let (source, user) = match &status.token {
        Token::Unknown => return fl!("gitlab-menu-looking"),
        Token::None if status.can_connect => return fl!("gitlab-menu-not-connected-note"),
        Token::None => return fl!("gitlab-menu-no-token-note"),
        Token::Found { source, user } => (*source, user),
    };
    let from = source_label(source, status);
    match user {
        None => fl!("gitlab-menu-checking", from = from),
        Some(Ok(user)) => fl!(
            "gitlab-menu-signed-in",
            user = user.username.as_str(),
            from = from
        ),
        Some(Err(error)) => match **error {
            gitlab::Error::Authentication if status.can_connect => {
                fl!("gitlab-menu-refused-connect", from = from)
            }
            gitlab::Error::Authentication => fl!("gitlab-menu-refused", from = from),
            // A pasted token is only tried after these, so it cannot help.
            gitlab::Error::EnvironmentRefused => fl!("gitlab-menu-environment-refused"),
            gitlab::Error::EnvrcRefused => fl!("gitlab-menu-envrc-refused", from = from),
            _ => fl!("gitlab-menu-failed", error = error.to_string(), from = from),
        },
    }
}

/// The instances listed, by normalized name, each with its API: a remote's
/// host becomes the instance whose `ssh_host` glab names it, and a saved
/// instance keeps glab's `api_host` for it. An instance is plain only when
/// every remote reads it over HTTP, since one HTTPS remote is enough to
/// send a token. A saved token adds an instance no remote reads, but never
/// changes how a listed one is read: its API defaults to HTTPS, which would
/// hide that the remotes withhold the token.
pub(crate) fn merge_hosts(
    listed: impl IntoIterator<Item = GitLabRead>,
    saved: impl IntoIterator<Item = String>,
    cli: &[CliHost],
) -> BTreeMap<String, Listed> {
    let mut hosts = BTreeMap::<String, Listed>::new();
    let listed = listed.into_iter().map(|read| Listed {
        api: Api::new(&read.host, read.scheme, cli),
        checkout: read.checkout,
    });
    let saved = saved.into_iter().map(|host| Listed {
        api: Api::of_instance(&host, cli),
        checkout: None,
    });
    let entries = listed
        .map(|entry| (entry, true))
        .chain(saved.map(|entry| (entry, false)));
    for (mut entry, read) in entries {
        entry.api.instance =
            saved::normalize_host(&entry.api.instance).unwrap_or(entry.api.instance);
        match hosts.get_mut(&entry.api.instance) {
            Some(_) if !read => {}
            Some(known) => {
                if known.api.scheme == Scheme::Http && entry.api.scheme == Scheme::Https {
                    known.api = entry.api;
                }
                if known.checkout.is_none() {
                    known.checkout = entry.checkout;
                }
            }
            None => {
                hosts.insert(entry.api.instance.clone(), entry);
            }
        }
    }
    if hosts.is_empty() {
        hosts.insert(
            DEFAULT_HOST.to_owned(),
            Listed {
                api: Api::of_instance(DEFAULT_HOST, cli),
                checkout: None,
            },
        );
    }
    hosts
}

/// An instance's token and whom it signs in as. Blocks on direnv, which
/// `stop` stops, `glab`, and the network: run it off the UI thread.
fn check_host(
    listed: &Listed,
    saved: impl FnOnce() -> Option<SecretString>,
    stop: &AtomicBool,
) -> Token {
    let api = &listed.api;
    let stopped = || stop.load(Ordering::Relaxed);
    match account::lookup_with(&api.instance, saved, listed.checkout.as_deref(), &stopped) {
        Some(found) => Token::Found {
            user: Some(
                account::whoami(api, &found.token)
                    .map_err(|error| Arc::new(error.from_source(found.source))),
            ),
            source: found.source,
        },
        None => Token::None,
    }
}

fn folder_name(checkout: &std::path::Path) -> Option<String> {
    Some(checkout.file_name()?.to_string_lossy().into_owned())
}

impl HerdrWindow {
    /// Shows the GitLab page: makes its fields, and looks up every
    /// instance's token.
    pub(crate) fn open_gitlab_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.page = Some(Page::GitLab);
        let field = |placeholder: &'static str, masked: bool, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_placeholder(placeholder, cx);
                input.set_masked(masked, cx);
            });
            input
        };
        if self.menu.gitlab.host.is_none() {
            self.menu.gitlab.host = Some(field(DEFAULT_HOST, false, cx));
        }
        if self.menu.gitlab.token.is_none() {
            self.menu.gitlab.token = Some(field("glpat-\u{2026}", true, cx));
        }
        for input in [&self.menu.gitlab.host, &self.menu.gitlab.token]
            .into_iter()
            .flatten()
        {
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
        }
        self.menu.gitlab.form = false;
        self.menu.gitlab.selected = None;
        window.focus(&self.menu.focus, cx);
        self.menu.gitlab.message = None;
        self.check_gitlab(cx);
        cx.notify();
    }

    /// Shows the token form at its token field, or with `add` at an empty
    /// instance field, for an instance no repository reads yet.
    pub(super) fn show_gitlab_form(
        &mut self,
        add: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let gitlab = &mut self.menu.gitlab;
        gitlab.form = true;
        gitlab.message = None;
        let field = if add { &gitlab.host } else { &gitlab.token };
        if let Some(input) = field {
            if add {
                input.update(cx, SearchInput::clear);
            }
            let focus = input.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(super) fn hide_gitlab_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.gitlab.form = false;
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    /// Loads the pictures of the accounts the page shows and takes those
    /// that finished. Returns whether one changed.
    pub(crate) fn poll_gitlab_avatars(&mut self) -> bool {
        let gitlab = &mut self.menu.gitlab;
        let urls = gitlab
            .hosts
            .iter()
            .filter_map(|status| match &status.token {
                Token::Found {
                    user: Some(Ok(user)),
                    ..
                } => user.avatar_url.as_deref(),
                _ => None,
            });
        gitlab.avatars.poll(urls)
    }

    /// The store saved tokens live in, or `None` in a headless window, which
    /// touches no saved credential.
    pub(super) fn gitlab_store(&self) -> Option<herdr_credentials::Store> {
        self.reads_credentials().then(|| self.config.gitlab.store())
    }

    /// [`Self::check_gitlab_hosts`] from inside this window's update, as
    /// opening the page is: GPUI leaves the window out of the other windows'
    /// read, so its own instances are added here.
    fn check_gitlab(&mut self, cx: &mut Context<Self>) {
        let mut listed = HerdrWindow::all_orchestrator_gitlab_hosts(cx);
        listed.extend(self.orchestrator_gitlab_hosts(cx));
        self.check_gitlab_hosts(listed, cx);
    }

    /// Lists `listed`, every main window's GitLab instances, with those that
    /// have a saved token, then looks each up on its own. Without a store
    /// the page still lists the instances, with no token.
    fn check_gitlab_hosts(&mut self, listed: Vec<GitLabRead>, cx: &mut Context<Self>) {
        self.menu.gitlab.preferred = self
            .orchestrator_gitlab_hosts(cx)
            .iter()
            .map(|read| saved::normalize_host(&read.host).unwrap_or_else(|| read.host.clone()))
            .collect();
        self.menu.gitlab.stop_lookups();
        let Some(store) = self.gitlab_store() else {
            self.menu.gitlab._task = None;
            self.menu.gitlab.problem = None;
            self.show_gitlab_hosts(&merge_hosts(listed, [], &[]), None, || Token::None);
            return;
        };
        self.show_gitlab_hosts(&merge_hosts(listed.clone(), [], &[]), None, || {
            Token::Unknown
        });
        let read = cx.background_spawn(async move { (saved::read(store), cli_hosts::read()) });
        self.menu.gitlab._task = Some(cx.spawn(async move |this, cx| {
            let (saved, cli) = read.await;
            let _ = this.update(cx, |this, cx| {
                this.menu.gitlab.problem = saved.as_ref().err().map(|error| match error {
                    gitlab::Error::Record => {
                        fl!("gitlab-menu-record-failed", error = error.to_string())
                    }
                    error => error.to_string(),
                });
                let saved = saved.unwrap_or_default();
                let hosts = merge_hosts(listed, saved.hosts().map(str::to_owned), &cli);
                this.show_gitlab_hosts(&hosts, Some(&saved), || Token::Unknown);
                this.menu.gitlab._lookups = hosts
                    .into_values()
                    .filter(|listed| listed.api.scheme == Scheme::Https)
                    .map(|listed| {
                        let kept = saved.kept(&listed.api.instance).cloned();
                        this.look_up_gitlab_host(listed, kept, store, cx)
                    })
                    .collect();
                this.finish_gitlab_check(cx);
                cx.notify();
            });
        }));
    }

    /// Lists `hosts`, each with `token`, except that a plain instance has
    /// none, and what `saved` keeps for it.
    fn show_gitlab_hosts(
        &mut self,
        hosts: &BTreeMap<String, Listed>,
        saved: Option<&Saved>,
        token: impl Fn() -> Token,
    ) {
        self.menu.gitlab.apis = hosts
            .iter()
            .map(|(host, listed)| (host.clone(), listed.api.clone()))
            .collect();
        let config = &self.config.gitlab;
        let mut last: BTreeMap<String, Token> = std::mem::take(&mut self.menu.gitlab.hosts)
            .into_iter()
            .filter(|status| !matches!(status.token, Token::Unknown))
            .map(|status| (status.host, status.token))
            .collect();
        let mut checking = BTreeSet::new();
        self.menu.gitlab.hosts = hosts
            .iter()
            .map(|(host, listed)| {
                let plain = listed.api.scheme == Scheme::Http;
                let kept = saved.and_then(|saved| saved.kept(host));
                let mut token = if plain { Token::None } else { token() };
                if matches!(token, Token::Unknown)
                    && let Some(found) = last.remove(host)
                {
                    checking.insert(host.clone());
                    token = found;
                }
                HostStatus {
                    saved: kept.is_some(),
                    connected: matches!(kept, Some(Kept::OAuth(_))),
                    can_connect: !plain && config.application(host).is_some(),
                    checkout: listed.checkout.as_deref().and_then(folder_name),
                    token,
                    host: host.clone(),
                    plain,
                }
            })
            .collect();
        self.menu.gitlab.checking = checking;
    }

    /// Looks one instance up off the UI thread. A connected account's token
    /// is read through the store, which renews it first when it is due.
    fn look_up_gitlab_host(
        &self,
        listed: Listed,
        kept: Option<Kept>,
        store: herdr_credentials::Store,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let host = listed.api.instance.clone();
        let stop = Arc::clone(&self.menu.gitlab.stop);
        let task = cx.background_spawn(async move {
            let token = check_host(
                &listed,
                || match kept {
                    Some(Kept::OAuth(_)) => account::kept(store, &listed.api.instance),
                    Some(kept) => Some(kept.token().clone()),
                    None => None,
                },
                &stop,
            );
            // A stopped lookup skipped the .envrc, so its answer may be wrong.
            (!stop.load(Ordering::Relaxed)).then_some(token)
        });
        cx.spawn(async move |this, cx| {
            let Some(token) = task.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.menu.gitlab.checking.remove(&host);
                if let Some(status) = this
                    .menu
                    .gitlab
                    .hosts
                    .iter_mut()
                    .find(|status| status.host == host)
                {
                    status.token = token;
                }
                this.finish_gitlab_check(cx);
                cx.notify();
            });
        })
    }

    /// Once every instance is looked up, an empty host field starts at the
    /// first instance still needing a token, this window's first.
    fn finish_gitlab_check(&mut self, cx: &mut Context<Self>) {
        let gitlab = &self.menu.gitlab;
        if !gitlab.checking.is_empty()
            || gitlab
                .hosts
                .iter()
                .any(|status| matches!(status.token, Token::Unknown))
        {
            return;
        }
        let Some(input) = &gitlab.host else {
            return;
        };
        if !input.read(cx).text().is_empty() {
            return;
        }
        let needs = |status: &&HostStatus| {
            !status.plain
                && !matches!(
                    status.token,
                    Token::Found {
                        user: Some(Ok(_)),
                        ..
                    }
                )
        };
        let Some(status) = gitlab
            .hosts
            .iter()
            .filter(needs)
            .find(|status| gitlab.preferred.contains(&status.host))
            .or_else(|| gitlab.hosts.iter().find(needs))
        else {
            return;
        };
        let host = status.host.clone();
        input.update(cx, |input, cx| input.set_text_selected(&host, cx));
    }

    /// Checks the pasted token with GitLab, then saves it for the instance typed.
    pub(super) fn save_gitlab(&mut self, cx: &mut Context<Self>) {
        let gitlab = &self.menu.gitlab;
        let (Some(host_input), Some(token_input)) = (&gitlab.host, &gitlab.token) else {
            return;
        };
        let typed = host_input.read(cx).text().trim().to_owned();
        let typed = if typed.is_empty() {
            DEFAULT_HOST.to_owned()
        } else {
            typed
        };
        let token = token_input.read(cx).text().trim().to_owned();
        let failure = match saved::normalize_host(&typed) {
            None => Some(gitlab::Error::Host(typed.clone()).to_string()),
            // A token sent to an HTTP address could be read on the way, and
            // an instance only read over HTTP would never send it.
            Some(host)
                if typed.to_ascii_lowercase().starts_with("http://")
                    || gitlab
                        .hosts
                        .iter()
                        .any(|status| status.plain && status.host == host) =>
            {
                Some(gitlab::Error::InsecureHost(host).to_string())
            }
            Some(_) if token.is_empty() => Some(fl!("gitlab-menu-paste-first")),
            Some(host) => {
                // The pasted token is the user's later choice: an approval
                // still awaited for the instance would replace it, maybe
                // with another account, so that wait stops.
                if gitlab
                    .connecting
                    .as_ref()
                    .is_some_and(|connecting| connecting.host == host)
                {
                    self.menu.gitlab.connecting = None;
                }
                self.gitlab_store().is_none().then(no_store)
            }
        };
        if let Some(failure) = failure {
            self.menu.gitlab.message = Some(failure);
            cx.notify();
            return;
        }
        let (Some(host), Some(store)) = (saved::normalize_host(&typed), self.gitlab_store()) else {
            return;
        };
        let token = SecretString::from(token);
        // Checked where the listed instance is read from; an instance no
        // remote reads goes where glab sends its API.
        let listed = self.menu.gitlab.apis.get(&host).cloned();
        // The field keeps the token until it is saved: GitLab shows a token
        // only once, so a failed check must not lose it.
        self.run_gitlab(cx, Some(token.clone()), move || {
            let api = listed.unwrap_or_else(|| Api::of_instance(&host, &cli_hosts::read()));
            let user = account::whoami(&api, &token)?.username;
            saved::save(store, &host, Some(token))?;
            Ok(fl!(
                "gitlab-menu-saved",
                host = host.as_str(),
                user = user.as_str()
            ))
        });
    }

    pub(super) fn forget_gitlab(&mut self, host: String, cx: &mut Context<Self>) {
        let Some(store) = self.gitlab_store() else {
            self.menu.gitlab.message = Some(no_store());
            cx.notify();
            return;
        };
        self.run_gitlab(cx, None, move || {
            saved::save(store, &host, None)?;
            Ok(fl!("gitlab-menu-removed", host = host.as_str()))
        });
    }

    /// Runs a save or forget off the UI thread, then says how it went, looks
    /// every instance up again, and has every window's Orchestrator read
    /// GitLab again. A successful save also empties the token field, unless
    /// another token was typed there meanwhile.
    pub(super) fn run_gitlab(
        &mut self,
        cx: &mut Context<Self>,
        saved_token: Option<SecretString>,
        work: impl FnOnce() -> Result<String, gitlab::Error> + Send + 'static,
    ) {
        self.menu.gitlab.working = true;
        self.menu.gitlab.message = None;
        let task = cx.background_spawn(async move { work() });
        self.menu.gitlab._save = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let changed = result.is_ok();
            let _ = this.update(cx, |this, cx| {
                this.menu.gitlab.working = false;
                if result.is_ok() && saved_token.is_some() {
                    this.menu.gitlab.form = false;
                }
                if result.is_ok()
                    && let Some(saved) = &saved_token
                    && let Some(input) = &this.menu.gitlab.token
                    && input.read(cx).text().trim() == saved.expose_secret()
                {
                    input.update(cx, SearchInput::clear);
                }
                this.menu.gitlab.message = Some(match result {
                    Ok(done) => done,
                    Err(error) => error.to_string(),
                });
                cx.notify();
            });
            Self::gitlab_changed(&this, changed, cx);
        }));
        cx.notify();
    }

    /// After a save, forget, or connection finished in the background:
    /// every window's views sync again when a token `changed`, and the page
    /// looks its instances up again. It runs between updates, never inside
    /// this window's own: GPUI panics when an entity it is updating is read
    /// or updated again.
    pub(super) fn gitlab_changed(this: &WeakEntity<Self>, changed: bool, cx: &mut AsyncApp) {
        if changed {
            cx.update(HerdrWindow::refresh_all_orchestrators);
        }
        let listed = cx.update(|cx| HerdrWindow::all_orchestrator_gitlab_hosts(cx));
        let _ = this.update(cx, |this, cx| this.check_gitlab_hosts(listed, cx));
    }
}

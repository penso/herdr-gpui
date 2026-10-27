//! The window's GitHub session: what is stored, what is in flight, and what a
//! worker thread has finished. Work runs off the UI thread and lands through
//! a single mailbox, so a late reply from a superseded flow is discarded.

#[cfg(any(test, feature = "test-support"))]
use super::VERIFY_URL;
use super::{
    Account, Device, Profile, Reply, Result, SETUP_MESSAGE, Store, cli,
    device::{with_access, with_organizations},
    http::oauth,
    log, profile, save,
    store::save_github_cli,
    token_reply, valid_token,
};
use crate::Error;
use secrecy::{ExposeSecret, SecretString};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

pub(super) struct Flow {
    pub(super) copied_until: Option<Instant>,
    pub(super) client: String,
    pub(super) device: Arc<Device>,
    pub(super) interval: u64,
    pub(super) deadline: Instant,
    pub(super) next: Instant,
}

const SESSION_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// How long the GitHub CLI's answer stands before the page asks it again.
const CLI_PROBE_INTERVAL: Duration = Duration::from_secs(30);
/// How soon an account waiting on GitHub App approval is checked again once
/// the user looks at it.
const ACCESS_RECHECK: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Auth {
    pub(super) flow: Option<Flow>,
    pub(super) incoming: Option<mpsc::Receiver<Result<Reply>>>,
    pub(super) cancelled: bool,
    pub(super) committing: bool,
    pub(super) signout_pending: bool,
    pub(super) initialized: bool,
    pub(super) signed_out: bool,
    pub(super) reload_pending: bool,
    pub(super) store: Store,
    pub(super) account: Account,
    pub(super) profile_incoming: Option<mpsc::Receiver<Result<Option<Profile>>>>,
    pub(super) last_session_check: Option<Instant>,
    pub(super) next_session_check: Option<Instant>,
    pub profile: Option<Profile>,
    pub message: Option<String>,
    pub failed: bool,
    pub(super) credential_cleanup: bool,
    /// When the GitHub CLI last answered, and whether it held a sign-in.
    pub(super) cli_available: Option<(Instant, bool)>,
    pub(super) cli_probe: Option<mpsc::Receiver<bool>>,
    /// The user chose the GitHub CLI in this app session. Only a build
    /// without a credential store reads it; the others keep a marker.
    pub(super) github_cli: bool,
    pub(super) stopping_github_cli: bool,
}

impl Auth {
    /// A saved host's own sign-in. It starts empty; `initialize` loads it.
    pub fn for_account(account: Account) -> Self {
        Self {
            account,
            ..Self::default()
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn complete_profile_fixture(&mut self, result: Result<Option<Profile>>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let _ = tx.send(result);
        self.profile_incoming = Some(rx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn connected_fixture() -> Self {
        Self {
            initialized: true,
            last_session_check: Some(Instant::now()),
            profile: Some(Profile {
                login: "fixture-user".into(),
                avatar_url: String::new(),
                token: Arc::new("fixture-token".into()),
                source: super::Source::Token,
                organizations: None,
                access: Default::default(),
            }),
            ..Self::default()
        }
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn access_recheck_scheduled_fixture(&self) -> bool {
        self.next_session_check.is_some()
    }
    pub fn connected(&self) -> bool {
        self.profile.is_some()
    }
    pub fn loading_profile(&self) -> bool {
        !self.signed_out && (self.reload_pending || self.profile_incoming.is_some())
    }
    pub fn initialize(&mut self, config: &crate::config::GitHubConfig) -> bool {
        self.initialize_with(super::store::select(config))
    }
    pub(super) fn initialize_with(&mut self, store: Store) -> bool {
        let changed = !self.initialized || self.store != store;
        self.store = store;
        self.initialized = true;
        if !changed || self.signed_out {
            return false;
        }
        self.profile = None;
        self.next_session_check = None;
        self.flow = None;
        self.cancelled = true;
        self.reload_pending = true;
        self.failed = false;
        // A host's first load is routine; the main account keeps its notice.
        self.message = (self.account == Account::Main)
            .then(|| "Checking GitHub account under the updated credential policy...".into());
        // Drain old workers before reloading, so rapid policy changes stay bounded.
        // Neither a late profile nor an accepted write can restore the old session.
        true
    }
    pub(super) fn load_profile_with(
        &mut self,
        token: Option<Arc<SecretString>>,
        load: impl FnOnce(Option<Arc<SecretString>>, Store) -> Result<Option<Profile>> + Send + 'static,
    ) {
        self.failed = false;
        let store = self.store;
        let (tx, rx) = mpsc::sync_channel(1);
        match thread::Builder::new()
            .name("herdr-github-profile".into())
            .spawn(move || {
                let _ = tx.send(load(token, store));
            }) {
            Ok(_) => self.profile_incoming = Some(rx),
            Err(error) => {
                tracing::error!(category = "github_worker", error_kind = ?error.kind(), "Could not start GitHub profile worker");
                self.failed = true;
                self.message = Some("Could not start GitHub profile worker.".into());
            }
        }
    }
    /// A main account whose GitHub CLI answered with a sign-in: offered
    /// while signed out, or already in use when `connected`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn github_cli_fixture(connected: bool) -> Self {
        let mut auth = if connected {
            Self::connected_fixture()
        } else {
            Self::fixture(false)
        };
        auth.cli_available = Some((Instant::now(), true));
        if let Some(profile) = &mut auth.profile {
            profile.source = super::Source::GitHubCli { repo_scope: false };
        }
        auth
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn fixture(waiting: bool) -> Self {
        let now = Instant::now();
        Self {
            flow: waiting.then(|| Flow {
                copied_until: None,
                client: "fixture".into(),
                device: Arc::new(Device {
                    device_code: "fixture".into(),
                    user_code: "ABCD-1234".into(),
                    verification_uri: VERIFY_URL.into(),
                    expires_in: 900,
                    interval: 900,
                }),
                interval: 900,
                deadline: now + Duration::from_secs(900),
                next: now + Duration::from_secs(900),
            }),
            message: Some(
                if waiting {
                    "Open GitHub and enter this code. Waiting for authorization..."
                } else {
                    SETUP_MESSAGE
                }
                .into(),
            ),
            ..Self::default()
        }
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn requesting_fixture() -> Self {
        let (_, incoming) = mpsc::sync_channel(1);
        Self {
            incoming: Some(incoming),
            initialized: true,
            message: Some("Requesting GitHub sign-in code...".into()),
            ..Self::default()
        }
    }
    pub fn busy(&self) -> bool {
        self.incoming.is_some() || self.flow.is_some() || self.signout_pending
    }
    pub fn code(&self) -> Option<&str> {
        // The device user code is intentionally displayed, unlike access tokens.
        self.flow
            .as_ref()
            .filter(|f| Instant::now() < f.deadline)
            .map(|f| f.device.user_code.expose_secret())
    }
    pub fn can_sign_out(&self) -> bool {
        self.connected() || self.credential_cleanup
    }
    pub fn copied(&self) -> bool {
        self.flow.as_ref().is_some_and(|flow| {
            flow.copied_until
                .is_some_and(|until| Instant::now() < until)
        })
    }
    pub fn copy_code(&mut self) -> Option<&str> {
        let flow = self.flow.as_mut().filter(|f| Instant::now() < f.deadline)?;
        flow.copied_until = Some(Instant::now() + Duration::from_secs(3));
        Some(flow.device.user_code.expose_secret())
    }
    pub(super) fn launch(&mut self, work: impl FnOnce() -> Result<Reply> + Send + 'static) {
        let (tx, rx) = mpsc::sync_channel(1);
        match thread::Builder::new()
            .name("herdr-github-auth".into())
            .spawn(move || {
                let _ = tx.send(work());
            }) {
            Ok(_) => self.incoming = Some(rx),
            Err(error) => {
                tracing::error!(category = "github_worker", error_kind = ?error.kind(), "Could not start GitHub authentication worker");
                self.flow = None;
                self.committing = false;
                self.failed = true;
                self.message = Some("Could not start GitHub authentication worker.".into());
            }
        }
    }
    /// Refuse a sign-in that would have nowhere to be saved, saying why.
    fn refuse_without_store(&mut self) -> bool {
        if self.store != Store::Environment {
            return false;
        }
        tracing::warn!(
            category = "github_signin",
            store = ?self.store,
            "No secure credential store is configured for GitHub sign-in"
        );
        self.failed = true;
        // The plaintext opt-in only exists where a private file can be kept
        // private, so platforms without one must not be told to set it.
        self.message = Some(if super::store::FILE {
            "No secure credential store configured. To accept unencrypted token storage, set [github] allow_plaintext_credentials = true and reload GUI config. Otherwise use GH_TOKEN / GITHUB_TOKEN.".into()
        } else {
            "No GitHub credential store is available on this platform. Sign in by setting GH_TOKEN or GITHUB_TOKEN.".into()
        });
        true
    }
    pub fn start(&mut self, config: &crate::config::GitHubConfig) {
        if self.busy() || self.connected() || self.loading_profile() {
            return;
        }
        self.store = super::store::select(config);
        self.initialized = true;
        self.failed = false;
        if self.refuse_without_store() {
            return;
        }
        let client = match config.client_id() {
            Ok(Some(client)) => client,
            Ok(None) => {
                tracing::info!(
                    category = "github_signin",
                    "No GitHub OAuth client ID is configured"
                );
                self.message = Some(SETUP_MESSAGE.into());
                return;
            }
            Err(error) => {
                log::failure("client_id", &error);
                self.failed = true;
                self.message = Some(error.to_string());
                return;
            }
        };
        self.cancelled = false;
        self.signed_out = false;
        self.github_cli = false;
        self.profile_incoming = None;
        self.message = Some("Requesting GitHub sign-in code...".into());
        tracing::info!(
            category = "github_signin",
            store = ?self.store,
            "Requesting a GitHub device code"
        );
        self.launch(move || {
            let started = Instant::now();
            let device = oauth::<Device>(
                "device/code",
                // OAuth Apps use repo; GitHub Apps ignore scope and use their
                // registered permissions and installation repository access.
                &[("client_id", &client), ("scope", "repo")],
                Duration::from_secs(15),
            )?
            .validate()?;
            Ok(Reply::Device(device, client, started))
        });
    }
    /// Whether to offer the GitHub CLI's sign-in: the main account only, as
    /// the local CLI is this computer's account, and only once it answered.
    pub fn github_cli_available(&self) -> bool {
        self.account == Account::Main && self.cli_available.is_some_and(|(_, ready)| ready)
    }
    /// Ask the GitHub CLI whether it is signed in to github.com, at most every
    /// 30 seconds while signed out. The token it prints is dropped unread.
    pub fn probe_github_cli(&mut self, now: Instant) {
        self.probe_github_cli_with(now, || cli::token().is_ok());
    }
    pub(super) fn probe_github_cli_with(
        &mut self,
        now: Instant,
        probe: impl FnOnce() -> bool + Send + 'static,
    ) {
        if self.account != Account::Main
            || self.connected()
            || self.busy()
            || self.loading_profile()
            || self.cli_probe.is_some()
            || self
                .cli_available
                .is_some_and(|(at, _)| now.duration_since(at) < CLI_PROBE_INTERVAL)
        {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        match thread::Builder::new()
            .name("herdr-github-cli".into())
            .spawn(move || {
                let _ = tx.send(probe());
            }) {
            Ok(_) => self.cli_probe = Some(rx),
            Err(error) => {
                tracing::warn!(category = "github_worker", error_kind = ?error.kind(), "Could not start GitHub CLI check");
                self.cli_available = Some((now, false));
            }
        }
    }
    /// Use the GitHub CLI's sign-in for the main account. The worker reads
    /// and verifies its token, then saves only a marker, never the token.
    pub fn use_github_cli(&mut self, config: &crate::config::GitHubConfig) {
        let store = super::store::select(config);
        self.use_github_cli_with(store, move || {
            let profile = cli::verify(cli::token()?)?;
            save_github_cli(store)?;
            Ok(Reply::GitHubCli(profile))
        });
    }
    pub(super) fn use_github_cli_with(
        &mut self,
        store: Store,
        work: impl FnOnce() -> Result<Reply> + Send + 'static,
    ) {
        if self.account != Account::Main
            || self.busy()
            || self.connected()
            || self.loading_profile()
        {
            return;
        }
        self.store = store;
        self.initialized = true;
        self.failed = false;
        self.cancelled = false;
        self.signed_out = false;
        self.profile_incoming = None;
        // The worker may save the marker, so it is not cancelled midway.
        self.committing = true;
        self.message = Some("Checking GitHub CLI sign-in...".into());
        tracing::info!(
            category = "github_signin",
            store = ?self.store,
            "Using the GitHub CLI's sign-in"
        );
        self.launch(work);
    }
    /// Sign in with a token the user pasted, such as a personal access token
    /// for an account Herdr's GitHub App cannot reach. GitHub checks it
    /// before it is saved, so a mistyped token is never stored.
    pub fn use_token(&mut self, token: SecretString, config: &crate::config::GitHubConfig) {
        let store = super::store::select(config);
        let account = self.account.clone();
        self.use_token_with(store, move || {
            let token = token.expose_secret().trim();
            if !valid_token(token) {
                return Err(Error::Token);
            }
            let token: SecretString = token.into();
            let profile = profile(Arc::new(token.clone()))?;
            save(Some(&token), store, &account)?;
            Ok(Reply::Pasted(complete(profile, &account)))
        });
    }
    pub(super) fn use_token_with(
        &mut self,
        store: Store,
        work: impl FnOnce() -> Result<Reply> + Send + 'static,
    ) {
        if self.busy() || self.connected() || self.loading_profile() {
            return;
        }
        self.store = store;
        self.initialized = true;
        self.failed = false;
        if self.refuse_without_store() {
            return;
        }
        self.cancelled = false;
        self.signed_out = false;
        self.github_cli = false;
        self.profile_incoming = None;
        // The worker may save the token, so it is not cancelled midway.
        self.committing = true;
        self.message = Some("Checking the token with GitHub...".into());
        tracing::info!(
            category = "github_signin",
            store = ?self.store,
            "Signing in with a pasted token"
        );
        self.launch(work);
    }
    pub fn cancel(&mut self) {
        // A token is persisted only after the UI accepts the completed flow. Once
        // accepted, Keychain writes finish off-thread even if the menu closes.
        if self.committing || self.signout_pending {
            return;
        }
        self.cancelled = true;
        self.failed = false;
        self.flow = None;
        self.message = Some("GitHub sign-in cancelled.".into());
    }
    pub fn sign_out(&mut self) {
        self.credential_cleanup = true;
        self.stopping_github_cli = self.github_cli
            || self
                .profile
                .as_ref()
                .is_some_and(|profile| profile.source.github_cli());
        self.github_cli = false;
        self.failed = false;
        self.initialized = true;
        self.signed_out = true;
        self.next_session_check = None;
        self.reload_pending = false;
        self.profile = None;
        self.flow = None;
        // A profile worker can rotate and persist credentials. Drain it before
        // deletion, just like an accepted sign-in write, but suppress its result.
        if !self.committing {
            self.incoming = None;
        }
        self.signout_pending = true;
        self.cancelled = false;
        self.message = Some("Signed out locally. Removing saved GitHub credential...".into());
    }
    /// Check promptly while the account panel is open, including after the
    /// user returns from approving the App. Repeated calls coalesce, and
    /// session checks start at most once every 30 seconds.
    pub fn recheck_access(&mut self, now: Instant) {
        if self.loading_profile()
            || self.busy()
            || self.signed_out
            || self
                .profile
                .as_ref()
                .is_none_or(|profile| !profile.access.needs_attention())
        {
            return;
        }
        let soon = self
            .last_session_check
            .map_or(now, |last| (last + ACCESS_RECHECK).max(now));
        if self.next_session_check.is_none_or(|next| next > soon) {
            self.next_session_check = Some(soon);
        }
    }
    pub fn poll(&mut self) -> bool {
        let store = self.store;
        let account = self.account.clone();
        self.poll_with_store(move |token| save(token, store, &account))
    }
    /// Credential backend chosen for this build and configuration.
    pub fn store(&self) -> Store {
        self.store
    }
    pub(super) fn poll_with_store(
        &mut self,
        persist: impl FnOnce(Option<&SecretString>) -> Result<()> + Send + 'static,
    ) -> bool {
        let account = self.account.clone();
        let github_cli = self.github_cli;
        self.poll_with(persist, move |token, store| {
            let loaded = match token {
                Some(token) => profile(token).map(Some),
                None => super::store::load_profile(store, &account, github_cli),
            }?;
            Ok(loaded.map(|profile| complete(profile, &account)))
        })
    }
    pub(super) fn poll_with(
        &mut self,
        persist: impl FnOnce(Option<&SecretString>) -> Result<()> + Send + 'static,
        load: impl FnOnce(Option<Arc<SecretString>>, Store) -> Result<Option<Profile>> + Send + 'static,
    ) -> bool {
        self.poll_at(Instant::now(), persist, load)
    }

    pub(super) fn poll_at(
        &mut self,
        now: Instant,
        persist: impl FnOnce(Option<&SecretString>) -> Result<()> + Send + 'static,
        load: impl FnOnce(Option<Arc<SecretString>>, Store) -> Result<Option<Profile>> + Send + 'static,
    ) -> bool {
        if self.signout_pending && !self.committing && self.profile_incoming.is_none() {
            self.signout_pending = false;
            self.committing = true;
            self.launch(move || {
                persist(None)?;
                Ok(Reply::SignedOut)
            });
            return true;
        }
        if self.reload_pending && !self.busy() && self.profile_incoming.is_none() {
            self.reload_pending = false;
            self.cancelled = false;
            self.last_session_check = Some(now);
            self.load_profile_with(None, load);
            return true;
        }
        if self.connected()
            && !self.signed_out
            && !self.busy()
            && self.profile_incoming.is_none()
            && self.next_session_check.is_some_and(|next| now >= next)
        {
            // Resolve the store again under its transaction lock: another window
            // may already have rotated the single-use refresh token or signed out.
            self.next_session_check = Some(now + SESSION_CHECK_INTERVAL);
            self.last_session_check = Some(now);
            self.load_profile_with(None, load);
            return true;
        }
        let mut changed = false;
        if let Some(rx) = &self.cli_probe {
            let available = match rx.try_recv() {
                Ok(available) => Some(available),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(false),
            };
            if let Some(available) = available {
                self.cli_probe = None;
                self.cli_available = Some((now, available));
                changed = true;
            }
        }
        if let Some(rx) = &self.profile_incoming {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(Error::Worker("profile"))),
            };
            if let Some(result) = result {
                self.profile_incoming = None;
                if !self.reload_pending && !self.signed_out {
                    self.next_session_check = Some(now + SESSION_CHECK_INTERVAL);
                    match result {
                        Ok(mut profile) => {
                            if let (Some(old), Some(new)) = (&self.profile, &mut profile) {
                                new.keep_organizations(old);
                                new.keep_access(old);
                                if old.token.expose_secret() == new.token.expose_secret() {
                                    // PR caches use token identity to fence workers.
                                    new.token = old.token.clone();
                                }
                            }
                            self.profile = profile;
                            self.failed = false;
                            self.message = None;
                        }
                        Err(error) => {
                            log::failure("profile", &error);
                            self.credential_cleanup = true;
                            if matches!(
                                error,
                                Error::Authentication
                                    | Error::Expired
                                    | Error::Denied
                                    | Error::Authorization
                                    | Error::Cli(
                                        cli::Error::Missing
                                            | cli::Error::SignedOut
                                            | cli::Error::Rejected
                                    )
                            ) {
                                self.profile = None;
                            }
                            self.failed = true;
                            self.message = Some(error.to_string());
                        }
                    }
                }
                changed = true;
            }
        }
        if let Some(rx) = &self.incoming {
            let reply = match rx.try_recv() {
                Ok(reply) => Some(reply),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(Error::Worker("authentication"))),
            };
            if let Some(reply) = reply {
                self.incoming = None;
                changed = true;
                if self.signout_pending || self.reload_pending {
                    self.committing = false;
                    return true;
                }
                if !self.cancelled {
                    match reply {
                        Ok(Reply::Device(device, client, started)) => {
                            let now = Instant::now();
                            tracing::info!(
                                category = "github_signin",
                                expires_in = device.expires_in,
                                interval = device.interval,
                                "GitHub device code ready; waiting for authorization"
                            );
                            self.flow = Some(Flow {
                                copied_until: None,
                                client,
                                deadline: started + Duration::from_secs(device.expires_in),
                                next: now + Duration::from_secs(device.interval),
                                interval: device.interval,
                                device: Arc::new(device),
                            });
                            self.message = Some(
                                "Open GitHub and enter this code. Waiting for authorization..."
                                    .into(),
                            );
                        }
                        Ok(Reply::Pending(slow)) => {
                            if let Some(flow) = &mut self.flow {
                                if slow {
                                    flow.interval = flow.interval.saturating_add(5);
                                }
                                flow.next = Instant::now() + Duration::from_secs(flow.interval);
                            }
                        }
                        Ok(Reply::Token(token)) => {
                            if self
                                .flow
                                .as_ref()
                                .is_some_and(|f| Instant::now() < f.deadline)
                            {
                                self.committing = true;
                                self.credential_cleanup = true;
                                self.message = Some("Saving GitHub credential...".into());
                                tracing::info!(
                                    category = "github_signin",
                                    store = ?self.store,
                                    "GitHub authorized; saving the credential"
                                );
                                self.launch(move || {
                                    persist(Some(&token.encode()?))?;
                                    Ok(Reply::Authenticated(Arc::new(token.access_token)))
                                });
                            } else {
                                tracing::warn!(
                                    category = "github_signin",
                                    "GitHub authorized after the device code expired"
                                );
                                self.failed = true;
                                self.message = Some("GitHub code expired. Sign in again.".into());
                            }
                            self.flow = None;
                        }
                        Ok(Reply::SignedOut) => {
                            self.credential_cleanup = false;
                            self.committing = false;
                            let github_cli = std::mem::take(&mut self.stopping_github_cli);
                            self.message = Some(match self.account {
                                Account::Main if github_cli => "Stopped using GitHub CLI authentication. GitHub CLI stays signed in; run `gh auth logout` to sign it out. Environment tokens are suppressed until app restart.",
                                Account::Main => "Signed out for this app session. Saved credential removed. Environment tokens are suppressed until app restart; GitHub grants are not revoked.",
                                Account::Host(_) => "Signed out of this device's account. Saved credential removed; GitHub grants are not revoked. Pull requests here use your main account.",
                                Account::Additional(_) => "Signed out of this account. Saved credential removed; GitHub grants are not revoked. Its repositories use your main account.",
                            }.into());
                        }
                        Ok(Reply::Pasted(profile)) => {
                            self.committing = false;
                            self.credential_cleanup = true;
                            self.next_session_check = Some(now + SESSION_CHECK_INTERVAL);
                            self.last_session_check = Some(now);
                            self.profile = Some(profile);
                            self.failed = false;
                            self.message = None;
                        }
                        Ok(Reply::GitHubCli(profile)) => {
                            self.committing = false;
                            self.credential_cleanup = true;
                            self.github_cli = true;
                            self.next_session_check = Some(now + SESSION_CHECK_INTERVAL);
                            self.profile = Some(profile);
                            self.failed = false;
                            self.message = None;
                        }
                        Ok(Reply::Authenticated(token)) => {
                            self.committing = false;
                            self.message = Some("Loading GitHub profile...".into());
                            self.last_session_check = Some(now);
                            self.load_profile_with(Some(token), load);
                        }
                        Err(error) => {
                            log::failure("authentication", &error);
                            self.flow = None;
                            self.committing = false;
                            self.failed = true;
                            self.message = Some(error.to_string());
                        }
                    }
                }
            }
        }
        if let Some(flow) = &mut self.flow
            && flow
                .copied_until
                .is_some_and(|until| Instant::now() >= until)
        {
            flow.copied_until = None;
            changed = true;
        }
        if self.incoming.is_none()
            && let Some(flow) = &self.flow
        {
            let now = Instant::now();
            if now >= flow.deadline {
                tracing::warn!(
                    category = "github_signin",
                    "GitHub device code expired before authorization"
                );
                self.flow = None;
                self.failed = true;
                self.message = Some("GitHub code expired. Sign in again.".into());
                return true;
            }
            if now >= flow.next {
                let client = flow.client.clone();
                // Share secret ownership with the in-flight worker, not plaintext.
                let device = Arc::clone(&flow.device);
                let timeout = (flow.deadline - now).min(Duration::from_secs(15));
                self.launch(move || {
                    token_reply(
                        oauth(
                            "oauth/access_token",
                            &[
                                ("client_id", &client),
                                ("device_code", device.device_code.expose_secret()),
                                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                            ],
                            timeout,
                        )?,
                        &client,
                    )
                });
            }
        }
        changed
    }
}

/// A loaded account, with what its GitHub App installations still need and
/// what an additional account also needs: the organizations it covers.
fn complete(profile: Profile, account: &Account) -> Profile {
    let (profile, installed) = with_access(profile);
    match account {
        Account::Additional(_) => with_organizations(profile, installed),
        Account::Main | Account::Host(_) => profile,
    }
}

//! The account menu's Azure DevOps page: each organization the windows'
//! Orchestrators read or a token is saved for, the token it is read with and
//! where that comes from, whom Azure DevOps signs it in as, and a pasted
//! personal access token checked with the organization before it is saved.
//! Every lookup blocks on the credential store, `az`, or the network, so it
//! runs on the background executor and lands back here.

// i18n: migrated
use super::Page;
use crate::i18n::fl;
use crate::{HerdrWindow, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_orchestrator::azure::{
    self,
    account::{self, Account},
    saved,
    token::{self, Source},
};
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod keys;
pub(super) mod render;
#[cfg(test)]
mod tests;

/// What is known of an organization's token.
pub(crate) enum Lookup {
    /// Not looked up yet.
    Unknown,
    None,
    Found {
        /// The token the organization is read with, which may not be the
        /// saved one.
        source: Source,
        /// Whom Azure DevOps signs the token in as, once asked.
        account: Option<Result<Account, Arc<azure::Error>>>,
    },
}

/// One organization the page lists.
pub(crate) struct OrganizationStatus {
    pub(crate) organization: String,
    /// A token is saved for it, so Forget is offered even while
    /// `AZURE_DEVOPS_EXT_PAT` takes precedence over it.
    pub(crate) saved: bool,
    pub(crate) lookup: Lookup,
}

/// The page's state, kept while the menu closes so a reopened page shows
/// the last lookup at once.
#[derive(Default)]
pub(crate) struct AzureMenu {
    pub(crate) organizations: Vec<OrganizationStatus>,
    /// Set to stop the lookups under way, when the next check replaces them
    /// or the menu closes.
    stop: Arc<AtomicBool>,
    pub(super) organization: Option<Entity<SearchInput>>,
    pub(super) token: Option<Entity<SearchInput>>,
    /// A save or forget under way; the buttons wait for it.
    pub(crate) working: bool,
    pub(crate) message: Option<String>,
    /// Why the saved tokens could not be read, shown above the organizations.
    pub(crate) problem: Option<String>,
    /// The saved-token read under way, replaced by the next one.
    _task: Option<Task<()>>,
    /// Each organization's lookup, landing on its own row as it finishes.
    _lookups: Vec<Task<()>>,
    /// The save or forget under way, kept apart from lookups so its result
    /// is still shown when the page is opened again.
    _save: Option<Task<()>>,
}

impl AzureMenu {
    /// Stops the lookups under way; their rows keep what they show.
    pub(crate) fn stop_lookups(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stop = Arc::default();
        self._task = None;
        self._lookups.clear();
    }
}

/// What the page says in a window that keeps no saved credential.
fn no_store() -> String {
    fl!("azure-devops-menu-no-store")
}

/// Where a token came from, as the page says it.
fn source_label(source: Source) -> String {
    match source {
        Source::Environment => fl!("azure-devops-menu-from-environment"),
        Source::Saved => fl!("azure-devops-menu-from-saved"),
        Source::Cli => fl!("azure-devops-menu-from-cli"),
    }
}

/// How many projects a check found, as a card and the saved message say it.
pub(crate) fn projects_note(account: &Account) -> String {
    match &account.projects {
        Some(projects) => fl!("azure-devops-menu-projects", count = projects.len()),
        None => fl!("azure-devops-menu-projects-hidden"),
    }
}

/// What an organization's row says.
pub(crate) fn status_line(status: &OrganizationStatus) -> String {
    let (source, account) = match &status.lookup {
        Lookup::Unknown => return fl!("azure-devops-menu-looking"),
        Lookup::None => return fl!("azure-devops-menu-no-token"),
        Lookup::Found { source, account } => (*source, account),
    };
    let from = source_label(source);
    match account {
        None => fl!("azure-devops-menu-checking", from = from),
        Some(Ok(account)) => fl!(
            "azure-devops-menu-signed-in",
            from = from,
            projects = projects_note(account)
        ),
        Some(Err(error)) => match **error {
            azure::Error::Authentication => fl!("azure-devops-menu-refused", from = from),
            // A pasted token is only tried after it, so it cannot help.
            azure::Error::EnvironmentRefused => fl!("azure-devops-menu-environment-refused"),
            _ => fl!(
                "azure-devops-menu-failed",
                error = crate::i18n::errors::Localize::localize(&**error),
                from = from
            ),
        },
    }
}

/// The organizations listed, sorted: those the windows read, then those
/// with a saved token.
pub(crate) fn merge_organizations(
    read: impl IntoIterator<Item = String>,
    saved: impl IntoIterator<Item = String>,
) -> Vec<String> {
    read.into_iter()
        .chain(saved)
        .filter_map(|organization| saved::normalize_organization(&organization))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The page that creates a personal access token for `organization`.
pub(crate) fn token_page(organization: &str) -> Option<String> {
    let organization = saved::normalize_organization(organization)?;
    Some(format!(
        "https://{}/{organization}/_usersSettings/tokens",
        azure::HOST
    ))
}

/// An organization's token and whom it signs in as. Blocks on the store,
/// `az`, which `stop` stops, and the network: run it off the UI thread.
fn check_organization(
    organization: &str,
    store: herdr_credentials::Store,
    stop: &AtomicBool,
) -> Lookup {
    let stopped = || stop.load(Ordering::Relaxed);
    match token::lookup(organization, store, &stopped) {
        Some(found) => Lookup::Found {
            account: Some(
                account::check(organization, &found.token)
                    .map_err(|error| Arc::new(error.from_source(found.source))),
            ),
            source: found.source,
        },
        None => Lookup::None,
    }
}

impl HerdrWindow {
    /// Shows the Azure DevOps page: makes its fields, and looks up every
    /// organization's token.
    pub(crate) fn open_azure_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.page = Some(Page::AzureDevOps);
        let field = |placeholder: &'static str, masked: bool, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_placeholder(placeholder, cx);
                input.set_masked(masked, cx);
            });
            input
        };
        if self.menu.azure.organization.is_none() {
            // An example organization name, as Azure DevOps's documentation uses.
            self.menu.azure.organization = Some(field("contoso", false, cx)); // i18n: data
        }
        if self.menu.azure.token.is_none() {
            self.menu.azure.token = Some(field("\u{2022}\u{2022}\u{2022}\u{2022}", true, cx));
        }
        for input in [&self.menu.azure.organization, &self.menu.azure.token]
            .into_iter()
            .flatten()
        {
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
        }
        self.menu.azure.message = None;
        let mut read = HerdrWindow::all_orchestrator_azure_organizations(cx);
        let own = self.orchestrator_azure_organizations(cx);
        read.extend(own.iter().cloned());
        // The field starts at this window's first organization.
        if let (Some(first), Some(input)) = (own.first(), &self.menu.azure.organization)
            && input.read(cx).text().is_empty()
        {
            let first = first.clone();
            input.update(cx, |input, cx| input.set_text_selected(&first, cx));
        }
        if let Some(input) = &self.menu.azure.token {
            let focus = input.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        self.check_azure_organizations(read, cx);
        cx.notify();
    }

    /// The store saved tokens live in, or `None` in a headless window, which
    /// touches no saved credential.
    fn azure_store(&self) -> Option<herdr_credentials::Store> {
        self.reads_credentials()
            .then(|| self.config.azure_devops.store())
    }

    /// Lists `read`, every main window's organizations, with those that have
    /// a saved token, then looks each up on its own. Without a store the page
    /// still lists them, with no token.
    pub(crate) fn check_azure_organizations(&mut self, read: Vec<String>, cx: &mut Context<Self>) {
        self.menu.azure.stop_lookups();
        let Some(store) = self.azure_store() else {
            self.menu.azure.problem = None;
            self.show_azure_organizations(merge_organizations(read, []), &BTreeSet::new(), || {
                Lookup::None
            });
            return;
        };
        let task = cx.background_spawn(async move { saved::read(store) });
        self.menu.azure._task = Some(cx.spawn(async move |this, cx| {
            let saved = task.await;
            let _ = this.update(cx, |this, cx| {
                this.menu.azure.problem = saved.as_ref().err().map(|error| {
                    fl!(
                        "azure-devops-menu-record-failed",
                        error = crate::i18n::errors::Localize::localize(error)
                    )
                });
                let kept: BTreeSet<String> = saved
                    .map(|saved| saved.organizations().map(str::to_owned).collect())
                    .unwrap_or_default();
                let organizations = merge_organizations(read, kept.iter().cloned());
                this.show_azure_organizations(organizations.clone(), &kept, || Lookup::Unknown);
                this.menu.azure._lookups = organizations
                    .into_iter()
                    .map(|organization| this.look_up_azure(organization, store, cx))
                    .collect();
                cx.notify();
            });
        }));
    }

    /// Lists `organizations`, each with `lookup`, keeping a row's last
    /// lookup while a new one runs so it holds still.
    pub(crate) fn show_azure_organizations(
        &mut self,
        organizations: Vec<String>,
        kept: &BTreeSet<String>,
        lookup: impl Fn() -> Lookup,
    ) {
        let mut last: Vec<OrganizationStatus> = std::mem::take(&mut self.menu.azure.organizations);
        self.menu.azure.organizations = organizations
            .into_iter()
            .map(|organization| {
                let mut found = lookup();
                if matches!(found, Lookup::Unknown)
                    && let Some(index) = last
                        .iter()
                        .position(|status| status.organization == organization)
                {
                    found = last.swap_remove(index).lookup;
                }
                OrganizationStatus {
                    saved: kept.contains(&organization),
                    organization,
                    lookup: found,
                }
            })
            .collect();
    }

    fn look_up_azure(
        &self,
        organization: String,
        store: herdr_credentials::Store,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let stop = Arc::clone(&self.menu.azure.stop);
        let task = cx.background_spawn({
            let organization = organization.clone();
            async move {
                let lookup = check_organization(&organization, store, &stop);
                (!stop.load(Ordering::Relaxed)).then_some(lookup)
            }
        });
        cx.spawn(async move |this, cx| {
            let Some(lookup) = task.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(status) = this
                    .menu
                    .azure
                    .organizations
                    .iter_mut()
                    .find(|status| status.organization == organization)
                {
                    status.lookup = lookup;
                }
                cx.notify();
            });
        })
    }

    /// The organization typed, normalized, or why it cannot be one.
    fn typed_azure_organization(&self, cx: &App) -> Result<String, String> {
        let typed = self
            .menu
            .azure
            .organization
            .as_ref()
            .map(|input| input.read(cx).text().trim().to_owned())
            .unwrap_or_default();
        if typed.is_empty() {
            return Err(fl!("azure-devops-menu-organization-first"));
        }
        saved::normalize_organization(&typed).ok_or_else(|| {
            crate::i18n::errors::Localize::localize(&azure::Error::Organization(typed))
        })
    }

    /// Opens Azure DevOps's page for a new token in the organization typed.
    pub(super) fn open_azure_token_page(&mut self, cx: &mut Context<Self>) {
        match self.typed_azure_organization(cx) {
            Ok(organization) => {
                if let Some(url) = token_page(&organization) {
                    cx.open_url(&url);
                }
            }
            Err(failure) => self.menu.azure.message = Some(failure),
        }
        cx.notify();
    }

    /// Checks the pasted token with the organization typed, then saves it.
    pub(super) fn save_azure(&mut self, cx: &mut Context<Self>) {
        if self.menu.azure.working {
            return;
        }
        let token = self
            .menu
            .azure
            .token
            .as_ref()
            .map(|input| input.read(cx).text().trim().to_owned())
            .unwrap_or_default();
        let organization = match self.typed_azure_organization(cx) {
            Ok(_) if token.is_empty() => Err(fl!("azure-devops-menu-paste-first")),
            Ok(organization) => self
                .azure_store()
                .map(|store| (organization, store))
                .ok_or_else(no_store),
            Err(failure) => Err(failure),
        };
        let (organization, store) = match organization {
            Ok(found) => found,
            Err(failure) => {
                self.menu.azure.message = Some(failure);
                cx.notify();
                return;
            }
        };
        let token = SecretString::from(token);
        // The field keeps the token until it is saved: Azure DevOps shows a
        // token only once, so a failed check must not lose it.
        self.run_azure(cx, Some(token.clone()), move || {
            let checked = account::check(&organization, &token::Token::Personal(token.clone()))?;
            saved::save(store, &organization, Some(token))?;
            Ok(fl!(
                "azure-devops-menu-saved",
                organization = organization.as_str(),
                user = checked.name.as_str(),
                projects = projects_note(&checked)
            ))
        });
    }

    pub(super) fn forget_azure(&mut self, organization: String, cx: &mut Context<Self>) {
        let Some(store) = self.azure_store() else {
            self.menu.azure.message = Some(no_store());
            cx.notify();
            return;
        };
        self.run_azure(cx, None, move || {
            saved::save(store, &organization, None)?;
            Ok(fl!(
                "azure-devops-menu-removed",
                organization = organization.as_str()
            ))
        });
    }

    /// Runs a save or forget off the UI thread, then says how it went, looks
    /// every organization up again, and has every window's Orchestrator
    /// read again. A successful save empties the token field, unless another
    /// token was typed there meanwhile.
    pub(crate) fn run_azure(
        &mut self,
        cx: &mut Context<Self>,
        saved_token: Option<SecretString>,
        work: impl FnOnce() -> Result<String, azure::Error> + Send + 'static,
    ) {
        self.menu.azure.working = true;
        self.menu.azure.message = None;
        let task = cx.background_spawn(async move { work() });
        self.menu.azure._save = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let changed = result.is_ok();
            let _ = this.update(cx, |this, cx| {
                this.menu.azure.working = false;
                if result.is_ok()
                    && let Some(saved) = &saved_token
                    && let Some(input) = &this.menu.azure.token
                    && input.read(cx).text().trim() == saved.expose_secret()
                {
                    input.update(cx, SearchInput::clear);
                }
                this.menu.azure.message = Some(match result {
                    Ok(done) => done,
                    Err(error) => crate::i18n::errors::Localize::localize(&error),
                });
                cx.notify();
            });
            if changed {
                cx.update(HerdrWindow::refresh_all_orchestrators);
            }
            let read = cx.update(|cx| HerdrWindow::all_orchestrator_azure_organizations(cx));
            let _ = this.update(cx, |this, cx| this.check_azure_organizations(read, cx));
        }));
        cx.notify();
    }
}

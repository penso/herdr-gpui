//! State one window's menu owns: which page is open, what it targets, and the
//! daemon answers still outstanding. Every modal action is fenced by the
//! selection epoch and connection generation it was started under, so a reply
//! from a replaced connection can never be applied to the current one.

// i18n: migrated
use super::endpoint_error;
use super::{Page, WorkspaceMenuAction, WorkspaceTarget, git, github};
use crate::dialog_input::DialogInput;
use crate::i18n::fl;
use gpui::{App, Entity, FocusHandle, Pixels, Point, ScrollHandle, Subscription};
use herdr_client::protocol::ClientShellSnapshot;

/// What an open menu covers, as its last frame laid it out. Windows pages
/// draw above GPUI and would hide it, so the ones it covers hide instead.
/// Tests and native checks read it too, to find the menu in the window.
#[cfg(any(windows, test, feature = "integration-test"))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Cover {
    /// Not laid out yet.
    #[default]
    Unknown,
    /// A dialog dims the whole window.
    All,
    /// A popover or menu covers only its own panel.
    Panel(gpui::Bounds<Pixels>),
}

#[cfg(any(windows, test))]
impl Cover {
    /// Whether a page drawn at `page` would hide this menu. Before the menu
    /// is laid out nothing is, so pages keep showing for that one frame.
    pub(crate) fn covers(self, page: Option<gpui::Bounds<Pixels>>) -> bool {
        match self {
            Self::Unknown => false,
            Self::All => true,
            Self::Panel(panel) => page.is_some_and(|page| page.intersects(&panel)),
        }
    }

    /// This cover if it was measured for `open`, the page open now; one
    /// measured for another page is stale until this one is laid out.
    #[cfg(windows)]
    pub(crate) fn settled(self, measured_for: Option<Page>, open: Option<Page>) -> Self {
        if measured_for == open {
            self
        } else {
            Self::Unknown
        }
    }
}

pub(crate) struct MenuState {
    pub page: Option<Page>,
    pub(crate) application: Option<crate::menus::in_window::OpenMenu>,
    pub(crate) application_bar: crate::menus::in_window::Geometry,
    /// How the open popup, or the dimming a closed dialog left, moves.
    pub(crate) appear: super::appear::Appear,
    /// Written by the menu's layout, read when presenting Windows pages.
    #[cfg(any(windows, test, feature = "integration-test"))]
    pub(crate) cover: std::rc::Rc<std::cell::Cell<Cover>>,
    pub(super) device_setup: Option<super::devices::Setup>,
    /// The provider whose Settings tab Add Device last asked for. Unit tests
    /// record it rather than open a Settings window, which reads the disk.
    #[cfg(all(test, feature = "cloud"))]
    pub(super) settings_opened: Option<crate::cloud::CloudProvider>,
    /// Whether Add provider asked for Settings > Cloud devices itself.
    #[cfg(all(test, feature = "cloud"))]
    pub(super) cloud_settings_opened: bool,
    pub(super) wsl_setup: Option<super::devices::WslSetup>,
    /// The saved distribution the removal confirmation names.
    pub(super) wsl_remove: Option<String>,
    pub(super) session_edit: Option<super::sessions::Edit>,
    /// The folder the open-checkout question is about.
    pub(super) open_checkout: Option<std::path::PathBuf>,
    /// The address the open-link question is about.
    pub(super) open_link: Option<crate::terminal::ExternalUrl>,
    /// The batch the destroy-cloud-devices question is about. Dropping it
    /// unanswered cancels the batch.
    /// The batch the create-cloud-machines question is about.
    #[cfg(feature = "cloud")]
    pub(super) cloud_create:
        Option<crate::cloud::batch::Pending<crate::cloud::batch::create::Creation>>,
    #[cfg(feature = "cloud")]
    pub(super) cloud_destroy:
        Option<crate::cloud::batch::Pending<Vec<crate::cloud::batch::Target>>>,
    #[cfg(feature = "coder")]
    pub(super) coder: Option<super::devices::coder::Wizard>,
    /// The open pairing dialog. Dropping it cancels its pairing.
    pub(crate) remote_pairing: Option<crate::remote::PairingDialog>,
    /// The device the revoke question is about.
    pub(crate) remote_revoke: Option<crate::remote::Revoke>,
    pub(super) devices_scroll: ScrollHandle,
    /// The sessions list scrolls its own way; the two popups never share one.
    pub(crate) sessions_scroll: ScrollHandle,
    pub(crate) usage_scroll: ScrollHandle,
    // Selection epoch and connection generation fence captured modal actions.
    pub(super) endpoint_target: (u64, u64),
    pub anchor: Point<Pixels>,
    /// Ignore repeated right presses until the opening gesture is released.
    pub(crate) opening_right_click: bool,
    pub focus: FocusHandle,
    pub(super) selected: Option<usize>,
    pub(super) workspace_selected: Option<WorkspaceMenuAction>,
    pub(super) git_selected: Option<git::Row>,
    /// The merge dialog's method, and the pull request (with its head) it
    /// was opened for: a branch that moved since must be reviewed again.
    pub(super) merge_method: Option<crate::pull_request::MergeMethod>,
    pub(super) merge_target: Option<crate::pr_actions::Target>,
    pub(super) target: Option<WorkspaceTarget>,
    pub input: Option<DialogInput>,
    pub(crate) error: Option<String>,
    pub(super) deletion: Option<Deletion>,
    pub(super) close_check: Option<super::workspace_close::CloseCheck>,
    /// The correlated `worktree.create` or `worktree.open` request, so the dialog
    /// can report the daemon's answer and follow the returned workspace.
    pub(super) creation: Option<String>,
    /// The name a new tab or workspace dialog proposed. Submitting it
    /// unchanged, or nothing, leaves naming to the daemon, as Herdr does.
    pub(super) suggested_name: Option<String>,
    pub(super) worktree_open: Option<super::worktree_open::Picker>,
    pub(super) keybinds_scroll: ScrollHandle,
    pub(crate) keybinds_search: Option<Entity<crate::search_input::SearchInput>>,
    pub(super) shortcut_editor: Option<super::shortcut_editor::Editor>,
    pub(super) _keybinds_subscription: Option<Subscription>,
    pub(crate) themes: Option<crate::theme_picker::ThemePicker>,
    pub(crate) palette: Option<crate::palette::Palette>,
    /// Whether Go To shows its preview; Alt-P toggles it for the session.
    pub(crate) palette_preview: bool,
    /// What the last closed palette leaves for Resume Palette.
    pub(crate) palette_resume: Option<crate::palette::Resume>,
    pub(crate) code_search: Option<crate::code_search::CodeSearch>,
    pub(crate) close: Option<crate::close_modal::CloseConfirmation>,
    pub(crate) tab: Option<crate::tab_menu::TabMenu>,
    pub(crate) group: Option<crate::group_menu::GroupMenu>,
    pub(crate) host: Option<super::devices::HostMenu>,
    pub(crate) pane: Option<crate::pane_menu::PaneMenu>,
    /// The new worktree dialog's tabs and the GitHub listing behind them.
    pub(crate) worktree: Option<super::WorktreeSource>,
    /// The host a new worktree or workspace goes to, when another one could.
    pub(crate) dispatch: Option<crate::dispatch::Picker>,
    pub(crate) pr: crate::pull_request::Lookup,
    /// Also read by the sidebar, which paints each worktree's cached PR badge.
    pub(crate) pr_cache: crate::pull_request::Cache,
    pub(super) pr_cache_connection:
        Option<std::sync::Weak<std::sync::Mutex<crate::state::LiveState>>>,
    pub(super) pr_snapshot: Option<std::sync::Weak<ClientShellSnapshot>>,
    pub(crate) github: crate::github::Auth,
    /// Saved SSH devices' own accounts, keyed by endpoint ID. A device without
    /// one signed in looks up pull requests with `github`.
    pub(crate) github_hosts: std::collections::HashMap<String, crate::github::Auth>,
    /// One sign-in per `[github] accounts` entry, in config order.
    pub(crate) github_accounts: Vec<github::AdditionalAccount>,
    /// Every signed-in account's picture, by GitHub's avatar address.
    pub(crate) github_avatars: crate::avatars::ProfileAvatars,
    /// The additional account the GitHub page shows, by ID; `None` shows
    /// the device's own account.
    pub(super) github_account: Option<String>,
    /// An account whose config entry is being written, shown once it loads.
    pub(super) github_adding: Option<String>,
    /// The pasted-token field, while the GitHub page shows it.
    pub(super) github_token: Option<Entity<crate::search_input::SearchInput>>,
    /// Logins this window already toasted as waiting on GitHub App approval.
    pub(super) github_toasted: std::collections::HashSet<String>,
    /// The GitLab page's instances, fields, and work under way.
    pub(crate) gitlab: super::gitlab::GitLabMenu,
    /// Saved devices whose removal is running, by endpoint ID. Kept after
    /// success until the catalog drops the device, so its header pulses
    /// until it disappears; the confirmation closes as soon as it starts.
    pub(crate) removing_devices: std::collections::HashSet<String>,
    pub(super) github_selected: Option<github::Action>,
    pub(super) github_scroll: ScrollHandle,
    pub(super) pr_connection: Option<std::sync::Weak<std::sync::Mutex<crate::state::LiveState>>>,
    pub(crate) version_notice: Option<super::VersionNotice>,
}

pub(super) struct Deletion {
    pub(super) pending: Option<String>,
    pub(super) path: Option<String>,
    /// The repository's main checkout, for the archive script's environment.
    pub(super) root: Option<String>,
    pub(super) force: bool,
    /// Whether the checkout has an archive script to run first.
    pub(super) archive: crate::worktree_scripts::ArchiveCheck,
    /// The merged pull request that opened this dialog without being asked.
    pub(super) merged: Option<u64>,
}

impl Deletion {
    pub(super) fn new(pending: Option<String>, force: bool) -> Self {
        Self {
            pending,
            path: None,
            root: None,
            force,
            archive: crate::worktree_scripts::ArchiveCheck::Unread,
            merged: None,
        }
    }

    /// Confirming is a single keypress, so the dialog may only submit once the
    /// daemon has named the checkout, its lookup is no longer in flight, and
    /// the checkout is known to have an archive script or not.
    pub(super) fn ready(&self) -> bool {
        self.pending.is_none() && self.path.is_some() && self.archive.settled()
    }
}

/// A queued `worktree.remove`, retained even if its confirmation is dismissed.
/// It drives sidebar progress and reports a refusal in the matching open dialog
/// and the window's local error. A dirty checkout enables a force confirmation.
pub(crate) struct Removal {
    /// Same fence as the menu target: a response from a replaced connection is
    /// not this removal's.
    pub(super) endpoint: (u64, u64),
    pub(super) boot_id: String,
    pub(super) workspace: String,
    /// The request id, until the daemon answers.
    pub(super) pending: Option<String>,
    /// Set once the daemon refused the checkout as dirty.
    pub(super) force: bool,
}

impl Removal {
    /// Whether the daemon has yet to answer, rather than a refusal kept for
    /// its force confirmation.
    pub(crate) fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn pending_for(&self, endpoint: (u64, u64), boot_id: &str, workspace: &str) -> bool {
        self.pending.is_some()
            && self.endpoint == endpoint
            && self.boot_id == boot_id
            && self.workspace == workspace
    }
}

/// What a submitted dialog did, which decides whether it stays open.
pub(super) enum Submission {
    /// The request is queued and the dialog has nothing left to wait for.
    Queued { focus_changed: bool },
    /// The dialog stays open for a daemon answer it still needs: the checkout a
    /// removal must confirm, or the one a creation produced.
    Awaiting { focus_changed: bool },
}

impl Submission {
    pub(super) fn focus_changed(&self) -> bool {
        let (Submission::Queued { focus_changed } | Submission::Awaiting { focus_changed }) = self;
        *focus_changed
    }
}

impl MenuState {
    /// Whether a menu or dialog is open on `workspace`.
    pub(crate) fn open_on(&self, workspace: &str) -> bool {
        self.page.is_some()
            && self
                .target
                .as_ref()
                .is_some_and(|target| target.id == workspace)
    }

    /// The repository the open dialog's host picker ranks hosts for.
    pub(crate) fn dispatch_repo(&self) -> Option<String> {
        self.target
            .as_ref()?
            .worktree
            .as_ref()
            .map(|tree| tree.label.clone())
    }

    pub(super) fn apply_deletion_response(
        &mut self,
        id: &str,
        result: crate::state::DialogResponse,
    ) {
        let Some(deletion) = &mut self.deletion else {
            return;
        };
        if deletion.pending.as_deref() != Some(id) {
            return;
        }
        deletion.pending = None;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        if let Some(error) = response.get("error") {
            let (code, message) = endpoint_error(error);
            self.error = Some(format!("{code}: {message}"));
            return;
        }
        let result = &response["result"];
        let Some(target) = &self.target else {
            return;
        };
        if result["type"] == "worktree_list" {
            let entry = result["worktrees"].as_array().and_then(|entries| {
                let mut matches = entries
                    .iter()
                    .filter(|entry| entry["open_workspace_id"] == target.id);
                let entry = matches.next()?;
                (matches.next().is_none()
                    && entry["is_linked_worktree"] == true
                    && entry["is_bare"] == false)
                    .then_some(entry)
            });
            deletion.path = entry
                .and_then(|entry| entry["path"].as_str())
                .filter(|path| !path.is_empty())
                .map(str::to_owned);
            deletion.root = crate::worktree_scripts::main_checkout(result);
            if deletion.path.is_none() {
                self.error = Some(fl!("menu-state-no-linked-checkout"));
            }
        } else {
            self.error = Some(fl!("menu-state-unexpected-response"));
        }
    }

    pub fn new(cx: &App) -> Self {
        Self {
            page: None,
            application: None,
            application_bar: Default::default(),
            appear: Default::default(),
            #[cfg(any(windows, test, feature = "integration-test"))]
            cover: Default::default(),
            device_setup: None,
            #[cfg(all(test, feature = "cloud"))]
            settings_opened: None,
            #[cfg(all(test, feature = "cloud"))]
            cloud_settings_opened: false,
            wsl_setup: None,
            wsl_remove: None,
            session_edit: None,
            open_checkout: None,
            open_link: None,
            #[cfg(feature = "cloud")]
            cloud_create: None,
            #[cfg(feature = "cloud")]
            cloud_destroy: None,
            #[cfg(feature = "coder")]
            coder: None,
            remote_pairing: None,
            remote_revoke: None,
            devices_scroll: ScrollHandle::new(),
            sessions_scroll: ScrollHandle::new(),
            usage_scroll: ScrollHandle::new(),
            endpoint_target: (0, 0),
            anchor: Point::default(),
            opening_right_click: false,
            focus: cx.focus_handle(),
            selected: None,
            workspace_selected: None,
            git_selected: None,
            merge_method: None,
            merge_target: None,
            target: None,
            input: None,
            error: None,
            deletion: None,
            close_check: None,
            creation: None,
            suggested_name: None,
            worktree_open: None,
            keybinds_scroll: ScrollHandle::new(),
            keybinds_search: None,
            shortcut_editor: None,
            _keybinds_subscription: None,
            themes: None,
            palette: None,
            palette_resume: None,
            palette_preview: true,
            code_search: None,
            close: None,
            pr: Default::default(),
            // Every window reads and refreshes the same answers.
            pr_cache: cx
                .try_global::<crate::pull_request::Shared>()
                .map_or_else(Default::default, |shared| {
                    crate::pull_request::Cache::shared(shared.0.clone())
                }),
            pr_cache_connection: None,
            pr_snapshot: None,
            github: Default::default(),
            github_hosts: Default::default(),
            github_accounts: Vec::new(),
            github_avatars: Default::default(),
            github_account: None,
            github_adding: None,
            github_token: None,
            github_toasted: Default::default(),
            gitlab: Default::default(),
            removing_devices: Default::default(),
            github_selected: None,
            github_scroll: ScrollHandle::new(),
            pr_connection: None,
            version_notice: None,
            tab: None,
            group: None,
            host: None,
            pane: None,
            worktree: None,
            dispatch: None,
        }
    }

    pub fn reset(&mut self) {
        self.application = None;
        #[cfg(any(windows, test, feature = "integration-test"))]
        self.cover.set(Cover::Unknown);
        self.device_setup = None;
        self.wsl_setup = None;
        self.wsl_remove = None;
        self.session_edit = None;
        self.open_checkout = None;
        self.open_link = None;
        #[cfg(feature = "cloud")]
        {
            self.cloud_create = None;
            self.cloud_destroy = None;
        }
        #[cfg(feature = "coder")]
        {
            self.coder = None;
        }
        self.remote_pairing = None;
        self.remote_revoke = None;
        self.devices_scroll.set_offset(Point::default());
        self.sessions_scroll.set_offset(Point::default());
        self.usage_scroll.set_offset(Point::default());
        self.opening_right_click = false;
        self.tab = None;
        self.group = None;
        self.host = None;
        self.pane = None;
        self.github_selected = None;
        self.github_scroll.set_offset(Point::default());
        if self.github.busy() {
            self.github.cancel();
        }
        for account in &mut self.github_accounts {
            if account.auth.busy() {
                account.auth.cancel();
            }
        }
        self.github_account = None;
        self.github_adding = None;
        self.github_token = None;
        // Closing the menu stops a GitLab connection, as it does GitHub's.
        self.gitlab.connecting = None;
        self.gitlab.form = false;
        self.gitlab.stop_lookups();
        self.page = None;
        if let Some(palette) = self.palette.take() {
            self.palette_resume = Some(palette.resume());
        }
        self.code_search = None;
        self.shortcut_editor = None;
        self.selected = None;
        self.workspace_selected = None;
        self.git_selected = None;
        self.merge_method = None;
        self.merge_target = None;
        self.target = None;
        self.input = None;
        self.error = None;
        self.deletion = None;
        self.version_notice = None;
        self.close_check = None;
        self.creation = None;
        self.suggested_name = None;
        self.worktree_open = None;
        self.close = None;
        self.worktree = None;
        self.dispatch = None;
        self.pr.clear();
        self.pr_connection = None;
    }
}

#[cfg(test)]
mod cover_tests {
    use super::Cover;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn only_pages_under_a_menu_step_aside() {
        let at = |x: f32, width: f32| Bounds::new(point(px(x), px(0.)), size(px(width), px(400.)));
        // A sessions popover on the left, a page on the right.
        let popover = Cover::Panel(at(40., 300.));
        assert!(!popover.covers(Some(at(900., 700.))));
        assert!(popover.covers(Some(at(200., 700.))));
        // A dimmed dialog covers every page; an unlaid menu none yet.
        assert!(Cover::All.covers(Some(at(900., 700.))));
        assert!(!Cover::Unknown.covers(Some(at(0., 1900.))));
        // A page not drawn yet is under nothing.
        assert!(!popover.covers(None));
    }
}

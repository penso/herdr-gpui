//! State one window's menu owns: which page is open, what it targets, and the
//! daemon answers still outstanding. Every modal action is fenced by the
//! selection epoch and connection generation it was started under, so a reply
//! from a replaced connection can never be applied to the current one.

use super::endpoint_error;
use super::{Page, WorkspaceMenuAction, WorkspaceTarget, git, github};
use crate::dialog_input::DialogInput;
use gpui::{App, Entity, FocusHandle, Pixels, Point, ScrollHandle, Subscription};
use herdr_client::protocol::ClientShellSnapshot;

pub(crate) struct MenuState {
    pub page: Option<Page>,
    pub(super) device_setup: Option<super::devices::Setup>,
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
    pub(super) target: Option<WorkspaceTarget>,
    pub input: Option<DialogInput>,
    pub(super) error: Option<String>,
    pub(super) deletion: Option<Deletion>,
    pub(super) close_check: Option<super::workspace_close::CloseCheck>,
    /// The correlated `worktree.create` or `worktree.open` request, so the dialog
    /// can report the daemon's answer and follow the returned workspace.
    pub(super) creation: Option<String>,
    pub(super) worktree_open: Option<super::worktree_open::Picker>,
    pub(super) keybinds_scroll: ScrollHandle,
    pub(crate) keybinds_search: Option<Entity<crate::search_input::SearchInput>>,
    pub(super) _keybinds_subscription: Option<Subscription>,
    pub(crate) preferences_scroll: ScrollHandle,
    pub(crate) themes: Option<crate::theme_picker::ThemePicker>,
    pub(crate) palette: Option<crate::palette::Palette>,
    pub(crate) close: Option<crate::close_modal::CloseConfirmation>,
    pub(crate) tab: Option<crate::tab_menu::TabMenu>,
    pub(crate) host: Option<super::devices::HostMenu>,
    pub(crate) pane: Option<crate::pane_menu::PaneMenu>,
    /// The new worktree dialog's tabs and the GitHub listing behind them.
    pub(crate) worktree: Option<super::WorktreeSource>,
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
    /// Saved devices whose removal is running, by endpoint ID. Kept after
    /// success until the catalog drops the device, so its header pulses
    /// until it disappears; the confirmation closes as soon as it starts.
    pub(crate) removing_devices: std::collections::HashSet<String>,
    pub(super) github_selected: Option<github::Action>,
    pub(super) github_scroll: ScrollHandle,
    pub(super) pr_connection: Option<std::sync::Weak<std::sync::Mutex<crate::state::LiveState>>>,
}

pub(super) struct Deletion {
    pub(super) pending: Option<String>,
    pub(super) path: Option<String>,
    pub(super) force: bool,
}

impl Deletion {
    /// Confirming is a single keypress, so the dialog may only submit once the
    /// daemon has named the checkout and its lookup is no longer in flight.
    pub(super) fn ready(&self) -> bool {
        self.pending.is_none() && self.path.is_some()
    }
}

/// A queued `worktree.remove`. The dialog closes as soon as the request is
/// queued, because the daemon's own snapshot drops the workspace once the
/// removal lands; holding the popover open adds nothing. What still needs a
/// home is sidebar progress and a refusal, which becomes the window's local
/// error. A dirty checkout arms the next dialog with force.
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
            if deletion.path.is_none() {
                self.error = Some("Daemon did not identify a unique linked checkout. Dismiss and reopen the menu.".into());
            }
        } else {
            self.error = Some(
                "Unexpected daemon response. Review current workspace state before retrying."
                    .into(),
            );
        }
    }

    pub fn new(cx: &App) -> Self {
        Self {
            page: None,
            device_setup: None,
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
            target: None,
            input: None,
            error: None,
            deletion: None,
            close_check: None,
            creation: None,
            worktree_open: None,
            keybinds_scroll: ScrollHandle::new(),
            keybinds_search: None,
            _keybinds_subscription: None,
            preferences_scroll: ScrollHandle::new(),
            themes: None,
            palette: None,
            close: None,
            pr: Default::default(),
            pr_cache: Default::default(),
            pr_cache_connection: None,
            pr_snapshot: None,
            github: Default::default(),
            github_hosts: Default::default(),
            removing_devices: Default::default(),
            github_selected: None,
            github_scroll: ScrollHandle::new(),
            pr_connection: None,
            tab: None,
            host: None,
            pane: None,
            worktree: None,
        }
    }

    pub fn reset(&mut self) {
        self.device_setup = None;
        self.devices_scroll.set_offset(Point::default());
        self.sessions_scroll.set_offset(Point::default());
        self.usage_scroll.set_offset(Point::default());
        self.opening_right_click = false;
        self.tab = None;
        self.host = None;
        self.pane = None;
        self.github_selected = None;
        self.github_scroll.set_offset(Point::default());
        if self.github.busy() {
            self.github.cancel();
        }
        self.page = None;
        self.selected = None;
        self.workspace_selected = None;
        self.git_selected = None;
        self.target = None;
        self.input = None;
        self.error = None;
        self.deletion = None;
        self.close_check = None;
        self.creation = None;
        self.worktree_open = None;
        self.close = None;
        self.worktree = None;
        self.pr.clear();
        self.pr_connection = None;
    }
}

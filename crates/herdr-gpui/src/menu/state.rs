//! State one window's menu owns: which page is open, what it targets, and the
//! daemon answers still outstanding. Every modal action is fenced by the
//! selection epoch and connection generation it was started under, so a reply
//! from a replaced connection can never be applied to the current one.

use super::endpoint_error;
use super::{Page, WorkspaceTarget, chrome::OverlayLayer};
use crate::HerdrWindow;
use gpui_kit::{
    AppContext as _, Context, Entity, Pixels, Point, Subscription,
    component::{input::InputState, menu::PopupMenu},
};
use herdr_client::protocol::ClientShellSnapshot;

/// The kit popup menu a pointer-anchored page is showing.
pub(super) struct Popup {
    pub(super) menu: Entity<PopupMenu>,
    pub(super) anchor: Point<Pixels>,
    /// Which corner of the menu sits at `anchor`.
    pub(super) corner: gpui_kit::Anchor,
    pub(super) _dismiss: Subscription,
}

pub(crate) struct MenuState {
    pub page: Option<Page>,
    /// Hosts the popup menu and the kit dialog layer. A child view, so dialog
    /// builders can read this window's state after its own render returns.
    pub(crate) layer: Entity<OverlayLayer>,
    pub(super) popup: Option<Popup>,
    pub(super) device_setup: Option<super::devices::Setup>,
    // Selection epoch and connection generation fence captured modal actions.
    pub(super) endpoint_target: (u64, u64),
    pub anchor: Point<Pixels>,
    pub(super) target: Option<WorkspaceTarget>,
    /// The open dialog's text field, when it has one.
    pub input: Option<Entity<InputState>>,
    pub(super) _input_subscription: Option<Subscription>,
    pub(super) error: Option<String>,
    pub(super) deletion: Option<Deletion>,
    pub(super) close_check: Option<super::workspace_close::CloseCheck>,
    /// The correlated `worktree.create` or `worktree.open` request, so the dialog
    /// can report the daemon's answer and follow the returned workspace.
    pub(super) creation: Option<String>,
    pub(super) worktree_open: Option<super::worktree_open::Picker>,
    pub(crate) keybinds_search: Option<Entity<InputState>>,
    pub(crate) themes: Option<crate::theme_picker::ThemePicker>,
    pub(crate) palette: Option<crate::palette::Palette>,
    pub(crate) close: Option<crate::close_modal::CloseConfirmation>,
    pub(crate) tab: Option<crate::tab_menu::TabMenu>,
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

    pub fn new(cx: &mut Context<HerdrWindow>) -> Self {
        let window = cx.weak_entity();
        Self {
            page: None,
            layer: cx.new(|_| OverlayLayer::new(window)),
            popup: None,
            device_setup: None,
            endpoint_target: (0, 0),
            anchor: Point::default(),
            target: None,
            input: None,
            _input_subscription: None,
            error: None,
            deletion: None,
            close_check: None,
            creation: None,
            worktree_open: None,
            keybinds_search: None,
            themes: None,
            palette: None,
            close: None,
            pr: Default::default(),
            pr_cache: Default::default(),
            pr_cache_connection: None,
            pr_snapshot: None,
            github: Default::default(),
            pr_connection: None,
            tab: None,
            pane: None,
            worktree: None,
        }
    }

    /// Forgets whatever the menu was showing. The kit surfaces follow on the
    /// next poll (see `HerdrWindow::sync_menu_overlay`), or at once through
    /// `dismiss_menu`, which also has the window to close them with.
    pub fn reset(&mut self) {
        self.device_setup = None;
        self.popup = None;
        self.tab = None;
        self.pane = None;
        if self.github.busy() {
            self.github.cancel();
        }
        self.page = None;
        self.target = None;
        self.input = None;
        self._input_subscription = None;
        self.keybinds_search = None;
        self.error = None;
        self.deletion = None;
        self.close_check = None;
        self.creation = None;
        self.worktree_open = None;
        self.close = None;
        self.worktree = None;
        self.palette = None;
        self.pr.clear();
        self.pr_connection = None;
    }

    /// The open dialog's text, or empty when it has no field.
    pub(crate) fn input_text(&self, cx: &gpui_kit::App) -> String {
        self.input
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }
}

//! The GitLab page from the keyboard, as the GitHub page works: Tab and
//! Shift-Tab select the page's next or previous control, Enter or Space
//! uses it, and Escape closes. The token form keeps its own keys: Tab moves
//! between its fields, Enter saves, and Escape returns to the instances.

use super::{GitLabMenu, HostStatus, Token};
use crate::i18n::fl;
use crate::{HerdrWindow, menu::Page};
use gpui::{prelude::*, *};
use herdr_orchestrator::gitlab::account::Source;

/// Which way Tab moves the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Forward,
    Back,
}

/// What a key does on the GitLab page, apart from typing into its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitLabKey {
    /// Escape: folds the token form, else closes the menu.
    Dismiss,
    /// Enter: saves the token form, else uses the selected control.
    Enter,
    /// Space: uses the selected control; in a field it types a space.
    Space,
    /// Tab or Shift-Tab: the form's other field, else the next or previous
    /// control.
    Move(Step),
}

impl GitLabKey {
    fn of(keystroke: &Keystroke) -> Option<Self> {
        let modifiers = keystroke.modifiers;
        let bare = modifiers == Modifiers::default();
        let plain = !modifiers.control && !modifiers.platform && !modifiers.alt;
        Some(match keystroke.key.as_str() {
            "escape" => Self::Dismiss,
            "enter" if bare => Self::Enter,
            "space" if bare => Self::Space,
            "tab" if plain && modifiers.shift => Self::Move(Step::Back),
            "tab" if plain => Self::Move(Step::Forward),
            _ => return None,
        })
    }

    /// The key acts on the page even while a field of the token form has
    /// focus.
    fn leaves_field(self) -> bool {
        !matches!(self, Self::Space)
    }
}

/// A control on the GitLab page, as the keyboard selects it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// The account menu's GitHub tab.
    GitHub,
    /// Connect, or Connect again, for an instance.
    Connect(String),
    /// Disconnect or Forget what is saved for an instance.
    Remove(String),
    /// Open GitLab at the code being approved.
    Open,
    /// Copy the code being approved.
    Copy,
    /// Stop waiting for the approval.
    Cancel,
    /// The link that opens the token form.
    Paste,
    /// + Add instance.
    Add,
    Close,
}

/// The labels of an instance card's buttons, when it shows them: Connect
/// where an application can sign in and no account is, or Connect again
/// where GitLab refused the connected one; Disconnect or Forget for what is
/// saved here.
pub(super) fn card_buttons(status: &HostStatus) -> (Option<String>, Option<String>) {
    let signed_in = matches!(
        status.token,
        Token::Found {
            user: Some(Ok(_)),
            ..
        }
    );
    // A token from an .envrc or the shell is tried before a connected
    // account, so connecting would change nothing.
    let shadowed = matches!(
        status.token,
        Token::Found {
            source: Source::Envrc | Source::Environment,
            ..
        }
    );
    let refused = matches!(
        status.token,
        Token::Found {
            source: Source::Saved,
            user: Some(Err(_)),
        }
    );
    let connect = (status.can_connect && (!status.connected || refused) && !signed_in && !shadowed)
        .then(|| {
            if status.connected {
                fl!("gitlab-menu-connect-again")
            } else {
                fl!("gitlab-menu-connect")
            }
        });
    let remove = status.saved.then(|| {
        if status.connected {
            fl!("gitlab-menu-disconnect")
        } else {
            fl!("gitlab-menu-forget")
        }
    });
    (connect, remove)
}

impl GitLabMenu {
    /// No instance has a token, and some instance could take one: the page
    /// welcomes the user instead of listing the instances.
    pub(super) fn shows_welcome(&self) -> bool {
        !self.form
            && self.hosts.iter().any(|status| !status.plain)
            && self
                .hosts
                .iter()
                .all(|status| matches!(status.token, Token::None))
    }

    /// The page's controls in the order they show, which Tab follows.
    pub(crate) fn actions(&self) -> Vec<Action> {
        let mut actions = vec![Action::GitHub];
        // Open, Copy, and Cancel show under the instance being connected,
        // once GitLab gave a code.
        let approving = |host: &str, actions: &mut Vec<Action>| {
            if self
                .connecting
                .as_ref()
                .is_some_and(|connecting| connecting.host == host && connecting.code.is_some())
            {
                actions.extend([Action::Open, Action::Copy, Action::Cancel]);
            }
        };
        if self.shows_welcome() {
            if let Some(status) = self.hosts.get(self.welcome_lead()) {
                if status.can_connect {
                    actions.push(Action::Connect(status.host.clone()));
                }
                approving(&status.host, &mut actions);
            }
            actions.push(Action::Paste);
        } else if !self.form {
            for status in &self.hosts {
                let (connect, remove) = card_buttons(status);
                if connect.is_some() {
                    actions.push(Action::Connect(status.host.clone()));
                }
                if remove.is_some() {
                    actions.push(Action::Remove(status.host.clone()));
                }
                approving(&status.host, &mut actions);
            }
            actions.extend([Action::Paste, Action::Add]);
        }
        actions.push(Action::Close);
        actions
    }

    /// Whether the keyboard selected `action`.
    pub(crate) fn is_selected(&self, action: &Action) -> bool {
        self.selected.as_ref() == Some(action)
    }

    /// Whether `host`'s Connect and Remove may be used now.
    fn ready_for(&self, host: &str) -> bool {
        !self.working && self.connecting.is_none() && !self.checking.contains(host)
    }
}

impl HerdrWindow {
    /// Uses `action`, as clicking its control does; a control that waits
    /// for something does nothing.
    fn gitlab_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::GitHub => self.menu.page = Some(Page::GitHub),
            Action::Connect(host) if self.menu.gitlab.ready_for(&host) => {
                self.connect_gitlab(host, cx);
            }
            Action::Remove(host) if self.menu.gitlab.ready_for(&host) => {
                self.forget_gitlab(host, cx);
            }
            Action::Connect(_) | Action::Remove(_) => {}
            Action::Open => self.open_gitlab_approval(cx),
            Action::Copy => self.copy_gitlab_code(cx),
            Action::Cancel => self.cancel_gitlab_connect(cx),
            Action::Paste => self.show_gitlab_form(false, window, cx),
            Action::Add => self.show_gitlab_form(true, window, cx),
            Action::Close => self.dismiss_menu(window, cx),
        }
        cx.notify();
    }

    /// Keys while the GitLab page shows.
    pub(in crate::menu) fn gitlab_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = GitLabKey::of(&event.keystroke);
        let fields = [&self.menu.gitlab.host, &self.menu.gitlab.token];
        let focused = fields
            .iter()
            .flat_map(|field| field.as_ref())
            .find(|input| input.read(cx).focus.is_focused(window));
        if let Some(input) = focused
            && (input.read(cx).is_composing() || !key.is_some_and(GitLabKey::leaves_field))
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        let Some(key) = key else {
            return;
        };
        let gitlab = &self.menu.gitlab;
        if gitlab.form {
            match key {
                GitLabKey::Dismiss => self.hide_gitlab_form(window, cx),
                GitLabKey::Enter if !gitlab.working => self.save_gitlab(cx),
                GitLabKey::Move(_) => {
                    let next = match focused {
                        Some(input) if Some(input) == gitlab.host.as_ref() => &gitlab.token,
                        _ => &gitlab.host,
                    };
                    if let Some(input) = next {
                        let focus = input.read(cx).focus.clone();
                        window.focus(&focus, cx);
                    }
                }
                GitLabKey::Enter | GitLabKey::Space => {}
            }
            cx.notify();
            return;
        }
        match key {
            GitLabKey::Move(step) => {
                let actions = gitlab.actions();
                let at = gitlab
                    .selected
                    .as_ref()
                    .and_then(|selected| actions.iter().position(|action| action == selected));
                let next = match (at, step) {
                    (None, Step::Forward) => 0,
                    (None, Step::Back) => actions.len() - 1,
                    (Some(at), Step::Forward) => (at + 1) % actions.len(),
                    (Some(at), Step::Back) => (at + actions.len() - 1) % actions.len(),
                };
                self.menu.gitlab.selected = actions.get(next).cloned();
            }
            GitLabKey::Enter | GitLabKey::Space => {
                // A control that went away since it was selected is not used.
                if let Some(action) = gitlab
                    .selected
                    .clone()
                    .filter(|selected| gitlab.actions().contains(selected))
                {
                    self.gitlab_action(action, window, cx);
                }
            }
            GitLabKey::Dismiss => self.dismiss_menu(window, cx),
        }
        cx.notify();
    }
}

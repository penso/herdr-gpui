//! Browser tabs a control request asks for, through the Unix control socket.

use super::scope;
use crate::{
    HerdrWindow,
    browser::{
        Scope, TabId,
        groups::{GroupId, Pick, Shown},
    },
    control::{Opening, Placed, Placement, Target},
};
use gpui::*;

impl HerdrWindow {
    /// The endpoint and workspace a control request names, if this window
    /// shows it. `strict` requires the caller's own daemon; otherwise any
    /// endpoint showing the named workspace qualifies. Only the Unix control
    /// socket asks.
    pub(crate) fn control_target(
        &self,
        target: &Target<'_>,
        strict: bool,
    ) -> Option<(usize, String)> {
        self.endpoints
            .iter()
            .enumerate()
            .find_map(|(index, endpoint)| {
                if strict {
                    let matches = match target.daemon {
                        Some(daemon) => {
                            endpoint.connection.target.socket_path().ok().as_deref() == Some(daemon)
                        }
                        None => index == self.selected_endpoint,
                    };
                    if !matches {
                        return None;
                    }
                } else if target.workspace.is_none() {
                    return None;
                }
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let snapshot = live.snapshot.as_ref()?;
                let workspace = match target.workspace {
                    Some(id) => id,
                    None => snapshot.focused_workspace_id.as_deref()?,
                };
                snapshot
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.workspace_id == workspace)
                    .then(|| (index, workspace.to_owned()))
            })
    }

    /// Whether the host this window shows is the daemon at `daemon`, listing
    /// workspaces or not, connected or not. Window-wide commands act on the
    /// shown host, so only that one may stand for the caller's daemon.
    pub(crate) fn shows_daemon(&self, daemon: &std::path::Path) -> bool {
        self.endpoints
            .get(self.selected_endpoint)
            .is_some_and(|endpoint| {
                endpoint.connection.target.socket_path().ok().as_deref() == Some(daemon)
            })
    }

    /// Opens the tab a control request asks for, if this window shows its
    /// workspace. `None` leaves the request to another window.
    pub(crate) fn open_requested_browser_tab(
        &mut self,
        target: &Target<'_>,
        strict: bool,
        opening: &Opening<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Placed> {
        let (index, workspace) = self.control_target(target, strict)?;
        let Opening {
            location,
            focus,
            placement,
        } = *opening;
        // Splitting needs the workspace's groups on screen: a workspace the
        // window does not show yet gets an ordinary tab.
        let besides = placement == Placement::Beside
            && self.browser_key() == Some((scope(&self.endpoints[index]), workspace.clone()));
        let opened = self.open_agent_tab(
            index,
            &workspace,
            target.pane,
            location,
            focus && !besides,
            window,
            cx,
        );
        let Some(id) = opened else {
            return Some(Placed::Full);
        };
        let beside = besides && self.show_beside(id, target.pane, focus, window, cx);
        if besides && !beside && focus {
            self.show_agent_tab(index, &workspace, id, window, cx);
        }
        Some(Placed::Opened {
            workspace_id: workspace,
            placement: if beside {
                Placement::Beside
            } else {
                Placement::Tab
            },
        })
    }

    /// The group showing `pane`'s terminal right now. With no pane, the
    /// caller is outside Herdr and any terminal group will do. A caller whose
    /// terminal no group shows, because the user switched away from it, has
    /// none: a page beside some other terminal would leave the caller hidden
    /// and the keyboard with another agent.
    fn terminal_group(&self, pane: Option<&str>, cx: &App) -> Option<GroupId> {
        let Some(pane) = pane else {
            return self.primary_group();
        };
        let snapshot = self.live.snapshot.as_ref()?;
        let tab = &snapshot.panes.iter().find(|p| p.pane_id == pane)?.tab_id;
        self.group_slots()
            .into_iter()
            .map(|slot| slot.id)
            .find(|group| {
                self.group_shown(*group, cx) == Shown::Terminal
                    && self.group_focused_tab(*group).as_ref() == Some(tab)
            })
    }

    /// Shows page `id` in a group beside the caller's terminal: the group
    /// the page already belongs to, another existing group, or a new one
    /// split off to the terminal's right. The terminal's group stays in use
    /// and keeps the keyboard, so the user can go on typing to the agent.
    /// Without `focus`, the user's view stays as it was: the group they had
    /// in use keeps the keyboard, and a group already showing another tab
    /// only lists the page in its strip.
    /// Returns whether the page is beside the terminal.
    fn show_beside(
        &mut self,
        id: TabId,
        pane: Option<&str>,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(home) = self.terminal_group(pane, cx) else {
            return false;
        };
        let in_use = self.active_group();
        let pick = Pick::Page(id);
        let owner = self.ensure_layout().map(|layout| layout.owner(&pick));
        let slots = self.group_slots();
        let index = slots.iter().position(|slot| slot.id == home).unwrap_or(0);
        let neighbour = slots
            .get(index + 1)
            .or_else(|| index.checked_sub(1).and_then(|before| slots.get(before)))
            .map(|slot| slot.id);
        let group = match owner.filter(|owner| *owner != home).or(neighbour) {
            Some(group) => group,
            None => {
                self.split_group(home, window, cx);
                match self.active_group().filter(|group| *group != home) {
                    Some(group) => group,
                    None => return false,
                }
            }
        };
        let shown_anywhere = slots
            .iter()
            .any(|slot| self.group_pick(slot.id).as_ref() == Some(&pick));
        let busy = self.group_pick(group).is_some_and(|shown| shown != pick);
        if !focus && busy && !shown_anywhere {
            if let Some(layout) = self.ensure_layout() {
                layout.give(group, &pick);
            }
            cx.notify();
        } else {
            self.move_tab_to_group(pick, group, window, cx);
        }
        let keyboard = if focus { Some(home) } else { in_use };
        self.activate_group(keyboard.unwrap_or(home), window, cx);
        true
    }

    /// The endpoint of this window whose live snapshot lists `workspace` of
    /// `scope`, whether or not the window shows it now.
    pub(crate) fn endpoint_showing(&self, scope: &Scope, workspace: &str) -> Option<usize> {
        self.endpoints
            .iter()
            .enumerate()
            .find_map(|(index, endpoint)| {
                if &super::scope(endpoint) != scope {
                    return None;
                }
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                live.snapshot
                    .as_ref()?
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.workspace_id == workspace)
                    .then_some(index)
            })
    }
}

/// The text a control request reads from a page: `null` until the page has
/// loaded, then its visible text, bounded before it leaves the page. The
/// app's own script; nothing a caller sends is ever run.
#[cfg(any(target_os = "macos", windows))]
// i18n: data
const PAGE_TEXT_SCRIPT: &str = "(function () {\
  if (document.readyState !== 'complete') { return null; }\
  var body = document.body;\
  return body ? String(body.innerText).slice(0, 65536) : '';\
})()";

/// A control request's view of this window's native pages.
#[cfg(any(target_os = "macos", windows))]
impl HerdrWindow {
    /// Whether this window has made the page of tab `id`.
    pub(crate) fn has_page(&self, id: TabId) -> bool {
        self.browser.pages.contains(id)
    }

    /// Loads `location` in tab `id`'s page, if this window has made it or
    /// is building it.
    pub(crate) fn load_requested_page(&mut self, id: TabId, location: &crate::browser::Location) {
        self.browser.pages.load(id, location);
    }

    /// Goes back, or forward, in tab `id`'s page. Whether the window has it.
    pub(crate) fn page_history(&self, id: TabId, forward: bool) -> bool {
        if !self.has_page(id) {
            return false;
        }
        if forward {
            self.browser.pages.forward(id);
        } else {
            self.browser.pages.back(id);
        }
        true
    }

    /// Asks tab `id`'s page for its text, handed to `done` as JSON: `null`
    /// while the page is still loading. Whether the window has the page.
    pub(crate) fn page_text(
        &self,
        id: TabId,
        done: impl Fn(String) + Send + 'static,
    ) -> crate::Result<bool> {
        let Some(page) = self.browser.pages.page(id) else {
            return Ok(false);
        };
        page.raw()
            .evaluate_script_with_callback(PAGE_TEXT_SCRIPT, done)?;
        Ok(true)
    }
}

#[cfg(target_os = "macos")]
impl HerdrWindow {
    /// Takes a picture of tab `id`'s page as it shows now, handed to `done`
    /// as TIFF bytes. Whether one was asked for: the page must be on
    /// screen, where it was laid out last frame.
    pub(crate) fn page_screenshot(
        &self,
        id: TabId,
        done: impl FnOnce(Option<Vec<u8>>) + 'static,
    ) -> bool {
        let Some(page) = self.browser.pages.page(id) else {
            return false;
        };
        if !self.browser.pages.is_shown(id) {
            return false;
        }
        let Some(bounds) = self.browser.pages.drawn_at(id) else {
            return false;
        };
        let rect = crate::browser::annotate::Rect {
            x: 0.,
            y: 0.,
            width: f64::from(f32::from(bounds.size.width)),
            height: f64::from(f32::from(bounds.size.height)),
        };
        crate::browser::snapshot::capture(page.raw(), rect, done);
        true
    }
}

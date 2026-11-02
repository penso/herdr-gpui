//! Chats in herdr-core (Herdr 2) panes. herdr-core keeps no pane metadata
//! a program could report its endpoint through, so the window chooses the
//! endpoint itself when it starts `herdr-acp` in a terminal: it passes the
//! id in [`ENDPOINT_ENV`] and labels the terminal with it under [`TOKEN`].
//! herdr-core keeps the label, so this window finds the chat again after a
//! restart, as any other window does.
//!
//! The label is daemon data like Herdr's token: the address is derived from
//! the id in this user's own directory, and only terminals of the core on
//! this machine are attached.

use super::view::Place;
use super::{MAX_CHATS, Route, Surface, Target};
use crate::{
    HerdrWindow,
    browser::TabId,
    core_terminals::{CoreTerminalView, Purpose},
};
use herdr_backend::{Spawn, TerminalId, TerminalInfo};
use herdr_chat_protocol::{Agent, ENDPOINT_ENV, EndpointId, TOKEN};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

/// The variable naming the Herdr pane `herdr-acp` reports to.
const PANE_ENV: &str = "HERDR_PANE_ID";

/// The window's key for the chat in `terminal`. Herdr pane ids have no
/// `core:` workspace, and a clash is dropped rather than shared.
pub(crate) fn key(terminal: &TerminalId) -> String {
    format!("core:{terminal}")
}

/// The chats among `terminals`, in their order, one per terminal and at
/// most [`MAX_CHATS`]. A terminal on another machine this core added
/// (`box/t7`) runs its program there, out of reach.
pub(super) fn targets<'a>(terminals: impl IntoIterator<Item = &'a TerminalInfo>) -> Vec<Target> {
    let mut seen = HashSet::new();
    terminals
        .into_iter()
        .filter(|info| !info.id.as_str().contains('/'))
        .filter_map(|info| {
            let endpoint: EndpointId = info.labels.get(TOKEN)?.parse().ok()?;
            Some(Target {
                endpoint,
                existing: None,
                pane_id: key(&info.id),
                route: Route::Local,
                surface: Surface::Core(info.id.clone()),
            })
        })
        .filter(|target| seen.insert(target.pane_id.clone()))
        .take(MAX_CHATS)
        .collect()
}

/// `herdr` followed by as many of `core` as fit under [`MAX_CHATS`], each
/// whose key no Herdr chat already holds.
pub(super) fn with_core(mut herdr: Vec<Target>, core: Vec<Target>) -> Vec<Target> {
    for target in core {
        if herdr.len() >= MAX_CHATS {
            break;
        }
        if !herdr.iter().any(|known| known.pane_id == target.pane_id) {
            herdr.push(target);
        }
    }
    herdr
}

/// The terminal that runs `program` as a chat with `agent`, listening on
/// `endpoint` and labelled with it, with the `[chat.env]` variables `extra`.
pub(super) fn spawn(
    agent: Agent,
    program: &Path,
    endpoint: &EndpointId,
    extra: &BTreeMap<String, String>,
) -> Spawn {
    let mut env: Vec<(String, String)> = extra
        .iter()
        .filter(|(name, _)| name.as_str() != ENDPOINT_ENV && name.as_str() != PANE_ENV)
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    env.push((ENDPOINT_ENV.to_owned(), endpoint.to_string()));
    // A core started from a Herdr pane passes that pane on; the chat must
    // not report itself there.
    env.push((PANE_ENV.to_owned(), String::new()));
    Spawn {
        argv: vec![program.to_string_lossy().into_owned(), agent.to_string()],
        env,
        labels: BTreeMap::from([(TOKEN.to_owned(), endpoint.to_string())]),
        // herdr-core names are unique handles, so two chats with one agent
        // could not share one; the program titles its pane instead.
        ..Spawn::default()
    }
}

impl HerdrWindow {
    /// The chats in the panes of the focused workspace's herdr-core tabs.
    pub(super) fn core_chat_targets(&self) -> Vec<Target> {
        targets(self.core_terminals.chat_terminals())
    }

    /// Splits `tab`'s pane in use with a chat running `agent`, beside it in
    /// its folder.
    pub(crate) fn start_core_chat(
        &mut self,
        agent: Agent,
        tab: TabId,
    ) -> Result<(), super::StartError> {
        if !self.config.features.acp_chats {
            return Err(super::StartError::Disabled);
        }
        if self.chat_agent_missing(agent) {
            return Err(super::StartError::AgentMissing(agent));
        }
        let program = self.chats.program.found()?.to_owned();
        let at = self
            .core_terminals
            .chat_split_at(tab)
            .ok_or(super::StartError::NoWorkspace)?;
        let spawn = spawn(
            agent,
            &program,
            &EndpointId::random(),
            &self.config.chat.env,
        );
        self.chats.last_started = Some(agent);
        self.start_core_program(
            Purpose::Split {
                tab,
                at,
                axis: crate::core_terminals::panes::Axis::Row,
            },
            spawn,
        );
        Ok(())
    }

    /// `terminal`'s pane in `tab`: its view, with its chat drawn over it when
    /// one is attached, or the button that brings the chat back over a pane
    /// turned to its terminal. A mirror only watches, so it keeps the view.
    pub(crate) fn core_pane_body(
        &self,
        tab: TabId,
        terminal: &TerminalId,
        view: &gpui::Entity<CoreTerminalView>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use gpui::{IntoElement as _, ParentElement as _, Styled as _, div};
        let pane_id = key(terminal);
        let chat = self
            .core_terminals
            .drives(tab)
            .then(|| self.chats.pane(&pane_id))
            .flatten()
            .filter(|chat| chat.available());
        let Some(chat) = chat else {
            return view.clone().into_any_element();
        };
        // The cells' last size shapes the chat's rail; it covers the pane.
        let size = view
            .read(cx)
            .bounds()
            .map(|bounds| bounds.size)
            .unwrap_or_default();
        let overlay = if self.chats.raw(&pane_id) {
            self.render_chat_switch(&pane_id, Place::Fill(size), cx)
        } else {
            self.render_chat(chat, Place::Fill(size), cx)
        };
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .size_full()
            .child(view.clone())
            .child(overlay)
            .into_any_element()
    }

    /// The herdr-core tab whose pane the chat keyed `pane_id` is drawn
    /// over, when it is a herdr-core pane's.
    pub(crate) fn core_chat_tab(&self, pane_id: &str) -> Option<TabId> {
        match &self.chats.pane(pane_id)?.target.surface {
            Surface::Core(terminal) => self.core_terminals.tab_of(terminal),
            Surface::Herdr => None,
        }
    }

    /// What takes the keyboard in `terminal`'s pane of `tab` when its chat
    /// shows: an open viewer, the conversation being read, or the composer.
    /// None in a mirror, which shows no chat.
    pub(crate) fn core_chat_focus(
        &self,
        tab: TabId,
        terminal: &TerminalId,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> Option<gpui::FocusHandle> {
        let pane_id = key(terminal);
        if !self.core_terminals.drives(tab) || self.chats.raw(&pane_id) {
            return None;
        }
        let pane = self.chats.pane(&pane_id).filter(|pane| pane.available())?;
        if let Some(focus) = pane.viewer.as_ref().and_then(|viewer| viewer.open_focus()) {
            return Some(focus.clone());
        }
        if pane.reading(window) {
            return pane.text_focus.clone();
        }
        pane.input_focus(cx)
    }

    /// Moves the keyboard from a herdr-core pane's terminal view to the chat
    /// drawn over it, as when the chat attaches while the pane has focus.
    pub(super) fn follow_core_chat_focus(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((tab, terminal)) = self.core_terminals.focused_pane(window, cx) else {
            return;
        };
        if let Some(focus) = self.core_chat_focus(tab, &terminal, window, cx) {
            window.focus(&focus, cx);
        }
    }

    /// The key of the chat in the herdr-core pane that holds the keyboard,
    /// through its chat or its terminal view, in a tab that drives it.
    pub(crate) fn focused_core_chat_key(
        &self,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> Option<String> {
        let (tab, terminal) = self
            .focused_core_chat(window, cx)
            .or_else(|| self.core_terminals.focused_pane(window, cx))?;
        let pane_id = key(&terminal);
        (self.core_terminals.drives(tab) && self.chats.pane(&pane_id).is_some()).then_some(pane_id)
    }

    /// The herdr-core tab and pane whose chat holds the keyboard.
    pub(crate) fn focused_core_chat(
        &self,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> Option<(TabId, TerminalId)> {
        self.chats.panes().find_map(|pane| {
            let Surface::Core(terminal) = &pane.target.surface else {
                return None;
            };
            pane.focus_handles(cx)
                .any(|focus| focus.is_focused(window))
                .then(|| Some((self.core_terminals.tab_of(terminal)?, terminal.clone())))
                .flatten()
        })
    }

    /// Makes the pane under the chat keyed `pane_id` the one in use, as a
    /// click on its terminal does: Herdr's focused pane once its frame is
    /// ready for input, or the herdr-core tab's active one, which needs no
    /// Herdr connection.
    pub(super) fn focus_chat_surface(&mut self, pane_id: &str, cx: &mut gpui::Context<Self>) {
        let surface = self
            .chats
            .pane(pane_id)
            .map(|pane| pane.target.surface.clone());
        match surface {
            Some(Surface::Core(terminal)) => {
                if let Some(tab) = self.core_terminals.tab_of(&terminal)
                    && self.core_terminals.set_active(tab, terminal)
                {
                    cx.notify();
                }
            }
            Some(Surface::Herdr) if self.input_ready() => self.focus_clicked_pane(pane_id, cx),
            Some(Surface::Herdr) => {}
            None => {}
        }
    }

    /// The herdr-core tab `group` shows, else the active group, that a new
    /// chat splits rather than opening a Herdr tab.
    pub(crate) fn core_chat_group_tab(
        &self,
        group: Option<crate::browser::GroupId>,
        cx: &gpui::App,
    ) -> Option<TabId> {
        let group = group.or_else(|| self.active_group())?;
        match self.group_shown(group, cx) {
            crate::browser::Shown::Page(tab) => self.core_terminals.chat_split_at(tab).map(|_| tab),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

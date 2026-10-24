//! A tab strip's agent buttons: one click starts a chat with that agent in a
//! new tab of the strip's group. `[chat] launcher` places them; the strip's
//! "+" opens a terminal tab in every placement. Only agents installed on
//! the selected host show, and none while no chat can start there.

use super::HerdrWindow;
use crate::{
    browser::Slot,
    config::{ChatLauncher, corners},
    icons::AgentIcon,
    window::chrome_focus::Region,
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::Agent;

/// An agent button and the mark inside it.
const BUTTON: f32 = 24.;
const ICON: f32 = 14.;

/// Where in the strip a launcher draws apart from "+".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spot {
    /// Before the tabs.
    Leading,
    /// After the room that fills the row, so at its right end.
    Trailing,
}

impl HerdrWindow {
    /// The agents the strips offer: the installed ones, unless the launcher
    /// is off or no chat can start.
    fn launcher_agents(&self) -> Vec<Agent> {
        if self.config.chat.launcher == ChatLauncher::Off {
            return Vec::new();
        }
        self.chat_agents()
    }

    /// Whether `slot`'s launcher shows the agents it hides until hovered.
    fn launcher_open(&self, slot: Slot) -> bool {
        self.chats.launcher_open == Some(slot.id)
    }

    /// The launcher's part at `spot` of `slot`'s strip, when its placement
    /// puts one there.
    pub(super) fn strip_launcher(
        &self,
        slot: Slot,
        spot: Spot,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let agents = self.launcher_agents();
        if agents.is_empty() {
            return None;
        }
        match (self.config.chat.launcher, spot) {
            (ChatLauncher::Leading, Spot::Leading) => Some(
                self.launcher_group(slot, "chat-launcher-leading")
                    .children(self.agent_buttons(slot, &agents, cx))
                    .child(self.launcher_rule())
                    .into_any_element(),
            ),
            (ChatLauncher::Trailing, Spot::Trailing) => Some(
                self.launcher_group(slot, "chat-launcher-trailing")
                    .children(self.agent_buttons(slot, &agents, cx))
                    .into_any_element(),
            ),
            (ChatLauncher::Button, Spot::Trailing) => Some(self.launcher_button(slot, &agents, cx)),
            _ => None,
        }
    }

    /// The strip's "+", with the agents beside it when the placement puts
    /// them there: always, or while the pointer is over it.
    pub(super) fn strip_new_tab(
        &self,
        slot: Slot,
        plus: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agents = self.launcher_agents();
        if agents.is_empty() {
            return plus.into_any_element();
        }
        match self.config.chat.launcher {
            ChatLauncher::Beside => self
                .launcher_group(slot, "chat-launcher-beside")
                .child(plus)
                .child(self.launcher_rule())
                .children(self.agent_buttons(slot, &agents, cx))
                .into_any_element(),
            ChatLauncher::Plus => {
                let open = self.launcher_open(slot);
                self.launcher_hover(slot, "chat-launcher-plus", cx)
                    .child(plus)
                    .when(open, |group| {
                        group
                            .child(self.launcher_rule())
                            .children(self.agent_buttons(slot, &agents, cx))
                    })
                    .into_any_element()
            }
            ChatLauncher::Leading
            | ChatLauncher::Trailing
            | ChatLauncher::Button
            | ChatLauncher::Off => plus.into_any_element(),
        }
    }

    /// The row a placement's buttons stand in.
    fn launcher_group(&self, slot: Slot, name: &'static str) -> Div {
        div()
            .debug_selector(move || slot.selector(name))
            .flex()
            .flex_none()
            .items_center()
    }

    /// A row that tells the window when the pointer enters or leaves it, so
    /// it can show the agents it hides.
    fn launcher_hover(
        &self,
        slot: Slot,
        name: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(SharedString::from(slot.selector(name)))
            .debug_selector(move || slot.selector(name))
            .flex()
            .flex_none()
            .items_center()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let open = if *hovered {
                    Some(slot.id)
                } else {
                    this.chats.launcher_open.filter(|group| *group != slot.id)
                };
                if this.chats.launcher_open != open {
                    this.chats.launcher_open = open;
                    cx.notify();
                }
            }))
    }

    /// One button per agent, in the order the host listed them.
    fn agent_buttons(
        &self,
        slot: Slot,
        agents: &[Agent],
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        agents
            .iter()
            .map(|&agent| {
                self.agent_button(slot, agent, cx)
                    .size(px(BUTTON))
                    .justify_center()
            })
            .collect()
    }

    /// The button that starts a chat with `agent`, without its size, so the
    /// one-button launcher can add the agent's name.
    fn agent_button(&self, slot: Slot, agent: Agent, cx: &mut Context<Self>) -> Stateful<Div> {
        let selector = format!("chat-launch-{}", agent.name());
        let label = SharedString::from(format!("New {} chat", agent.display_name()));
        let (foreground, surface, hover) =
            (self.theme.foreground, self.theme.surface, self.theme.active);
        div()
            .id(SharedString::from(slot.selector(&selector)))
            .debug_selector(move || slot.selector(&selector))
            .role(Role::Button)
            .aria_label(label.clone())
            .map(|control| self.chrome_stop(Region::TitleBar, control))
            .flex_none()
            .flex()
            .items_center()
            .rounded(px(corners::CONTROL))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .tooltip(move |_, cx| {
                cx.new(|_| crate::usage::Hint {
                    text: label.clone(),
                    foreground,
                    surface,
                })
                .into()
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.start_chat_in(agent, slot.id, cx);
            }))
            .child(AgentIcon::from_identity(Some(agent.name())).element(ICON, foreground))
    }

    /// One button, named, for the agent last started (else the first), with
    /// the others sliding out before it while the pointer is over it.
    fn launcher_button(&self, slot: Slot, agents: &[Agent], cx: &mut Context<Self>) -> AnyElement {
        let first = self
            .chats
            .last_started
            .filter(|agent| agents.contains(agent))
            .unwrap_or(agents[0]);
        let others: Vec<Agent> = agents
            .iter()
            .copied()
            .filter(|agent| *agent != first)
            .collect();
        let open = self.launcher_open(slot);
        self.launcher_hover(slot, "chat-launcher-button", cx)
            .gap(px(2.))
            .when(open, |group| {
                group.children(self.agent_buttons(slot, &others, cx))
            })
            .child(
                self.agent_button(slot, first, cx)
                    .h(px(BUTTON))
                    .px(px(6.))
                    .gap(px(6.))
                    .border_1()
                    .border_color(rgb(self.theme.active))
                    .text_size(px(self.config.ui.size - 1.))
                    .text_color(rgb(self.theme.foreground))
                    .child(first.display_name()),
            )
            .into_any_element()
    }

    /// The thin rule between "+" or the tabs and the agents.
    fn launcher_rule(&self) -> Div {
        div()
            .flex_none()
            .mx(px(4.))
            .w(px(1.))
            .h(px(14.))
            .bg(rgb(self.theme.active))
    }
}

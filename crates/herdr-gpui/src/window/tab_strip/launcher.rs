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

mod deck;
#[cfg(test)]
pub(super) use deck::{card_offset, deal_toward, spread};

/// The one-button launcher's height, and an agent's mark.
const BUTTON: f32 = 24.;
const ICON: f32 = 14.;
/// How long hidden agents take to slide out.
const SLIDE: std::time::Duration = std::time::Duration::from_millis(180);

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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agents = self.launcher_agents();
        if agents.is_empty() {
            return plus.into_any_element();
        }
        // As tall as the strip, as it is without agents: classic's "+" draws
        // its rule and hover fill from top to bottom.
        let plus = plus.self_stretch();
        match self.config.chat.launcher {
            ChatLauncher::Beside => self
                .launcher_group(slot, "chat-launcher-beside")
                .child(plus)
                .children(self.agent_buttons(slot, &agents, cx))
                .into_any_element(),
            ChatLauncher::Plus => self.plus_deck(slot, plus, &agents, window, cx),
            ChatLauncher::Leading
            | ChatLauncher::Trailing
            | ChatLauncher::Button
            | ChatLauncher::Off => plus.into_any_element(),
        }
    }

    /// The row a placement's buttons stand in, as tall as the strip.
    fn launcher_group(&self, slot: Slot, name: &'static str) -> Div {
        div()
            .debug_selector(move || slot.selector(name))
            .flex()
            .flex_none()
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

    /// One button per agent, in the order the host listed them, each drawn
    /// as the layout draws "+" with the agent's mark in place of the plus.
    fn agent_buttons(
        &self,
        slot: Slot,
        agents: &[Agent],
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let layout = self.tab_layout();
        agents
            .iter()
            .map(|&agent| {
                layout.new_tab(
                    self.agent_button(slot, agent, cx),
                    self.agent_mark(agent),
                    &self.theme,
                )
            })
            .collect()
    }

    /// `agent`'s mark, in color.
    fn agent_mark(&self, agent: Agent) -> AnyElement {
        AgentIcon::from_identity(Some(agent.name())).element(ICON, self.theme.foreground)
    }

    /// The button that starts a chat with `agent`: its identity, tooltip,
    /// and click, for the caller to shape and fill.
    fn agent_button(&self, slot: Slot, agent: Agent, cx: &mut Context<Self>) -> Stateful<Div> {
        let selector = format!("chat-launch-{}", agent.name());
        let label = SharedString::from(format!("New {} chat", agent.display_name()));
        let (foreground, surface) = (self.theme.foreground, self.theme.surface);
        div()
            .id(SharedString::from(slot.selector(&selector)))
            .debug_selector(move || slot.selector(&selector))
            .role(Role::Button)
            .aria_label(label.clone())
            .map(|control| self.chrome_stop(Region::TitleBar, control))
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
        let hover = self.theme.active;
        self.launcher_hover(slot, "chat-launcher-button", cx)
            .gap(px(2.))
            .when(open && !others.is_empty(), |group| {
                let width = self.tab_layout().new_tab_width() * others.len() as f32;
                group.child(
                    self.launcher_slide(slot, width)
                        .children(self.agent_buttons(slot, &others, cx)),
                )
            })
            .child(
                self.agent_button(slot, first, cx)
                    .self_center()
                    .flex_none()
                    .flex()
                    .items_center()
                    .rounded(px(corners::CONTROL))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(hover)))
                    .h(px(BUTTON))
                    .px(px(6.))
                    .gap(px(6.))
                    .border_1()
                    .border_color(rgb(self.theme.active))
                    .text_size(px(self.config.ui.size - 1.))
                    .text_color(rgb(self.theme.foreground))
                    .child(self.agent_mark(first))
                    .child(first.display_name()),
            )
            .into_any_element()
    }

    /// The row the one-button launcher reveals, opening out of the button:
    /// it widens from nothing to `width`, clipping the buttons it has not
    /// reached yet, so they slide out from under it toward the tabs. GPUI
    /// restarts the animation whenever the row is drawn again after a frame
    /// without it.
    fn launcher_slide(&self, slot: Slot, width: f32) -> AnimationElement<Div> {
        div()
            .debug_selector(move || slot.selector("chat-launcher-slide"))
            .flex()
            .flex_none()
            .overflow_hidden()
            // The end by the button stays put; the far end moves out.
            .justify_end()
            .with_animation(
                SharedString::from(slot.selector("chat-launcher-slide")),
                Animation::new(crate::motion::duration(SLIDE)).with_easing(ease_out_quint()),
                move |row, delta| row.w(px(width * delta)).opacity(delta),
            )
    }
}

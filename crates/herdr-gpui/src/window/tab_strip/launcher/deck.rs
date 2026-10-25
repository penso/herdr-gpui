//! The default launcher: the agents lie stacked under "+" like a deck of
//! cards and are dealt out beside it while the pointer is over it, each
//! setting off a little after the one before; when the pointer leaves they
//! gather back under "+" in reverse. Each card is drawn as the layout draws
//! "+", so the deck reads as more of the strip's own buttons.

use super::HerdrWindow;
use crate::{
    browser::Slot,
    chat::{LauncherDeck, Toward},
    motion::Toggle,
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::Agent;

/// How far into the deal each card sets off after the one before, as a
/// share of one card's own trip.
const STAGGER: f32 = 0.35;

/// How far the `index`th of `count` cards has travelled, from 0 stacked
/// under "+" to 1 in its place, when the deck is `shown` dealt.
pub(in crate::window::tab_strip) fn spread(shown: f32, index: usize, count: usize) -> f32 {
    let total = 1. + STAGGER * count.saturating_sub(1) as f32;
    (shown * total - STAGGER * index as f32).clamp(0., 1.)
}

/// How far a `width` wide card stands from the deck's edge by "+" once it
/// has travelled `t`: from hidden under "+" to after the cards before it.
pub(in crate::window::tab_strip) fn card_offset(t: f32, index: usize, width: f32) -> f32 {
    width * ((index + 1) as f32 * t - 1.)
}

/// The side `count` cards `width` wide deal toward when the window has
/// `past` room after the middle of "+": the right while all of them fit.
pub(in crate::window::tab_strip) fn deal_toward(past: f32, width: f32, count: usize) -> Toward {
    if past >= width * (count as f32 + 0.5) {
        Toward::Right
    } else {
        Toward::Left
    }
}

/// What the pointer is over that keeps a deck dealt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Over {
    Plus,
    Cards,
}

impl HerdrWindow {
    /// `slot`'s deck, if it is dealt at all.
    fn deck(&self, slot: Slot) -> Option<LauncherDeck> {
        self.chats
            .launcher_deck
            .filter(|deck| deck.group == slot.id)
    }

    /// Notes the pointer entering or leaving "+" or the cards: the deck
    /// deals out while it is over either, and gathers back once over
    /// neither. Moving from one to the other keeps it dealt.
    fn deck_hover(
        &mut self,
        slot: Slot,
        over: Over,
        hovered: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // The pointer is over "+" as a deck starts to deal, so the room past
        // it is what is left of the window.
        let past = window.viewport_size().width - window.mouse_position().x;
        let toward = deal_toward(
            f32::from(past),
            self.tab_layout().new_tab_width(),
            self.launcher_agents().len(),
        );
        let deck = match &mut self.chats.launcher_deck {
            Some(deck) if deck.group == slot.id => deck,
            _ if !hovered => return,
            other => other.insert(LauncherDeck {
                group: slot.id,
                over_cards: false,
                over_plus: false,
                toggle: Toggle::default(),
                toward: Toward::default(),
            }),
        };
        match over {
            Over::Plus => deck.over_plus = hovered,
            Over::Cards => deck.over_cards = hovered,
        }
        let open = deck.over_plus || deck.over_cards;
        if deck.toggle.is_open() == open {
            return;
        }
        let now = crate::motion::now();
        // Only a deck gathered all the way picks its side afresh.
        if open && deck.toggle.shown(now) <= 0. {
            deck.toward = toward;
        }
        deck.toggle.set(open, now, false);
        cx.notify();
    }

    /// "+" with its deck of agents, dealt out while the pointer is over
    /// either.
    pub(super) fn plus_deck(
        &self,
        slot: Slot,
        plus: Stateful<Div>,
        agents: &[Agent],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = crate::motion::now();
        let deck = self.deck(slot);
        if deck.is_some_and(|deck| deck.toggle.moving(now)) {
            window.request_animation_frame();
        }
        let name = "chat-launcher-plus";
        let group = div()
            .id(SharedString::from(slot.selector(name)))
            .debug_selector(move || slot.selector(name))
            .relative()
            .flex()
            .flex_none()
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                this.deck_hover(slot, Over::Plus, *hovered, window, cx);
            }))
            .child(plus);
        let Some(deck) = deck.filter(|deck| deck.toggle.shown(now) > 0.) else {
            return group.into_any_element();
        };
        let shown = deck.toggle.shown(now);
        let width = self.tab_layout().new_tab_width();
        let count = agents.len();
        let travelled: Vec<f32> = (0..count)
            .map(|index| spread(shown, index, count))
            .collect();
        // The deck is as wide as its farthest card reaches.
        let reach = travelled
            .iter()
            .enumerate()
            .map(|(index, &t)| card_offset(t, index, width) + width)
            .fold(0., f32::max);
        // The theme's own surface, never the backdrop's translucent one, so
        // a card hides the cards and tabs beneath it.
        let fill = rgb(self.theme.surface);
        let cards = self
            .agent_buttons(slot, agents, cx)
            .into_iter()
            .zip(&travelled)
            .enumerate()
            .map(|(index, (card, &t))| {
                let offset = px(card_offset(t, index, width));
                let card = card.absolute().top_0().bottom_0().bg(fill);
                match deck.toward {
                    Toward::Right => card.left(offset),
                    Toward::Left => card.right(offset),
                }
            });
        // Dealt over whatever lies beside "+" rather than taking room, so
        // "+" never moves under the pointer and a click meant for it always
        // lands on it; drawn after the rest of the strip, so on top.
        let cards = div()
            .id(SharedString::from(slot.selector("chat-launcher-cards")))
            .debug_selector(move || slot.selector("chat-launcher-deck"))
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(reach.max(0.)))
            .map(|cards| match deck.toward {
                Toward::Right => cards.left(relative(1.)),
                Toward::Left => cards.right(relative(1.)),
            })
            .overflow_hidden()
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                this.deck_hover(slot, Over::Cards, *hovered, window, cx);
            }))
            // The first card dealt lies on top of the rest.
            .children(cards.rev());
        group.child(deferred(cards)).into_any_element()
    }
}

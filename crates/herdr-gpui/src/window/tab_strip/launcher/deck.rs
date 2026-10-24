//! The default launcher: the agents lie stacked under "+" like a deck of
//! cards and are dealt out to its right while the pointer is over it, each
//! setting off a little after the one before; when the pointer leaves they
//! gather back under "+" in reverse.

use super::{BUTTON, HerdrWindow, RULE};
use crate::{browser::Slot, chat::LauncherDeck, config::corners, motion::Toggle};
use gpui::{prelude::*, *};
use herdr_chat_protocol::Agent;

/// The room between two cards once dealt.
pub(in crate::window::tab_strip) const GAP: f32 = 4.;
/// How far into the deal each card sets off after the one before, as a
/// share of one card's own trip.
const STAGGER: f32 = 0.35;

/// How far the `index`th of `count` cards has travelled, from 0 stacked
/// under "+" to 1 in its place, when the deck is `shown` dealt.
pub(in crate::window::tab_strip) fn spread(shown: f32, index: usize, count: usize) -> f32 {
    let total = 1. + STAGGER * count.saturating_sub(1) as f32;
    (shown * total - STAGGER * index as f32).clamp(0., 1.)
}

/// Where a card's left edge stands from the deck's start once it has
/// travelled `t`: from hidden under "+" to after the rule and the cards
/// before it, with a gap after each.
pub(in crate::window::tab_strip) fn card_left(t: f32, index: usize) -> f32 {
    let rest = RULE + (BUTTON + GAP) * index as f32;
    -BUTTON + (rest + BUTTON) * t
}

/// What the pointer is over that keeps a deck dealt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Over {
    Plus,
    Cards,
}

impl HerdrWindow {
    /// How far `slot`'s deck is dealt at `now`, and whether it still moves.
    fn deck_shown(&self, slot: Slot, now: std::time::Instant) -> (f32, bool) {
        self.chats
            .launcher_deck
            .filter(|deck| deck.group == slot.id)
            .map_or((0., false), |deck| {
                (deck.toggle.shown(now), deck.toggle.moving(now))
            })
    }

    /// Notes the pointer entering or leaving "+" or the cards: the deck
    /// deals out while it is over either, and gathers back once over
    /// neither. Moving from one to the other keeps it dealt.
    fn deck_hover(&mut self, slot: Slot, over: Over, hovered: bool, cx: &mut Context<Self>) {
        let deck = match &mut self.chats.launcher_deck {
            Some(deck) if deck.group == slot.id => deck,
            _ if !hovered => return,
            other => other.insert(LauncherDeck {
                group: slot.id,
                over_cards: false,
                over_plus: false,
                toggle: Toggle::default(),
            }),
        };
        match over {
            Over::Plus => deck.over_plus = hovered,
            Over::Cards => deck.over_cards = hovered,
        }
        let open = deck.over_plus || deck.over_cards;
        if deck.toggle.is_open() != open {
            deck.toggle.set(open, crate::motion::now(), false);
            cx.notify();
        }
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
        let (shown, moving) = self.deck_shown(slot, now);
        if moving {
            window.request_animation_frame();
        }
        let name = "chat-launcher-plus";
        let group = div()
            .id(SharedString::from(slot.selector(name)))
            .debug_selector(move || slot.selector(name))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.deck_hover(slot, Over::Plus, *hovered, cx);
            }))
            .child(plus);
        if shown <= 0. {
            return group.into_any_element();
        }
        let count = agents.len();
        let travelled: Vec<f32> = (0..count)
            .map(|index| spread(shown, index, count))
            .collect();
        // The deck is as wide as its farthest card reaches.
        let width = travelled
            .iter()
            .enumerate()
            .map(|(index, &t)| card_left(t, index) + BUTTON)
            .fold(0., f32::max);
        // The theme's own surface, never the backdrop's translucent one, so
        // a card hides the cards beneath it.
        let fill = rgb(self.theme.surface);
        let edge = rgb(self.theme.active);
        let cards = self
            .agent_buttons(slot, agents, cx)
            .into_iter()
            .zip(&travelled)
            .enumerate()
            .map(|(index, (card, &t))| {
                card.absolute()
                    .top_0()
                    .left(px(card_left(t, index)))
                    .bg(fill)
                    .border_1()
                    .border_color(edge)
                    .rounded(px(corners::CONTROL))
            });
        // Dealt over whatever follows "+" rather than beside it, so "+"
        // never moves under the pointer and a click meant for it always
        // lands on it; drawn after the rest of the strip, so on top.
        let deck = div()
            .debug_selector(move || slot.selector("chat-launcher-deck"))
            .relative()
            .flex_none()
            .h(px(BUTTON))
            .w(px(width.max(0.)))
            .overflow_hidden()
            .child(
                self.launcher_rule()
                    .absolute()
                    .top(px((BUTTON - 14.) / 2.))
                    .left_0()
                    .opacity(shown),
            )
            // The first card dealt lies on top of the rest.
            .children(cards.rev());
        group
            .child(deferred(
                div()
                    .id(SharedString::from(slot.selector("chat-launcher-cards")))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(1.))
                    .flex()
                    .items_center()
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.deck_hover(slot, Over::Cards, *hovered, cx);
                    }))
                    .child(deck),
            ))
            .into_any_element()
    }
}

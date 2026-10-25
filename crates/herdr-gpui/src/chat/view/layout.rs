//! Chat views as swappable layouts.
//!
//! The view prepares what an entry shows, and the composer's parts, once,
//! and hands them to the configured [`ChatLayout`], which decides how an
//! entry's row reads (on a timeline, or as bubbles) and how the composer's
//! parts sit together, as the tab strip's layouts do for tabs. The view
//! keeps what the parts do (their listeners, menus, focus, and the list
//! itself), so a chat behaves the same in every layout.

mod bubbles;
mod cards;
mod document;
mod imessage;
mod terminal;
mod timeline;

use crate::{
    HerdrWindow,
    config::{ChatLayoutMode, FontConfig, Theme},
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Author, Entry, EntryKind};

/// Who an entry speaks for; a turn's summary speaks for neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    User,
    Agent,
}

pub(super) fn side(kind: &EntryKind) -> Option<Side> {
    match kind {
        EntryKind::User { .. }
        | EntryKind::Image {
            author: Author::User,
            ..
        } => Some(Side::User),
        EntryKind::Turn(_) => None,
        _ => Some(Side::Agent),
    }
}

/// An entry the view prepared, and who spoke around it, which decides
/// where a layout marks a change of speaker.
pub(super) struct Row<'a> {
    pub(super) entry: &'a Entry,
    pub(super) previous: Option<Side>,
    pub(super) next: Option<Side>,
    /// The agent's display name, as the program reports it.
    pub(super) agent: &'a str,
    /// The entry is the last while a turn runs, so it may still grow.
    pub(super) live: bool,
    pub(super) content: Div,
}

impl Row<'_> {
    pub(super) fn side(&self) -> Option<Side> {
        side(&self.entry.kind)
    }

    /// Whether this entry starts what its speaker says.
    pub(super) fn speaker(&self) -> bool {
        self.side().is_some() && self.previous != self.side()
    }
}

/// What a layout draws with.
pub(super) struct Look<'a> {
    pub(super) theme: &'a Theme,
    /// The chat's text size.
    pub(super) text: f32,
    /// The terminal's font, for code and for layouts that read like one.
    pub(super) mono: &'a FontConfig,
    window: &'a HerdrWindow,
}

impl Look<'_> {
    /// `content` over `color`, as `.bg(color)` would paint it, except where
    /// a page in a message shows from beneath the window's drawing.
    pub(super) fn ground(&self, content: Div, color: u32) -> AnyElement {
        self.window.with_ground(content, rgb(color))
    }

    /// For a background painted by hand: where it must paint around pages
    /// in a message, or `None` while none lies under it.
    pub(super) fn pages_under(
        &self,
    ) -> impl Fn(Bounds<Pixels>) -> Option<Vec<Bounds<Pixels>>> + 'static {
        self.window.pages_under()
    }
}

impl HerdrWindow {
    pub(super) fn chat_look(&self) -> Look<'_> {
        Look {
            theme: &self.theme,
            text: self.chat_text_size(),
            mono: &self.config.terminal,
            window: self,
        }
    }
}

/// The composer's parts, each already carrying its listeners.
pub(super) struct ComposerParts {
    /// The files to send, while there are any.
    pub(super) chips: Option<Div>,
    /// The draft's earlier lines and the editable one.
    pub(super) draft: Div,
    pub(super) attach: Stateful<Div>,
    /// The session's settings and the auto-accept switch.
    pub(super) settings: Vec<AnyElement>,
    /// Context use and cost, once the agent reports them.
    pub(super) usage: Option<Div>,
    pub(super) send: Stateful<Div>,
}

pub(super) trait ChatLayout {
    /// The entry's row in the list, centered in the reading column. A
    /// turn's summary takes [`summary_row`] in every layout.
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement;

    /// The composer's card, from its parts.
    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div;

    /// Where an agent message's copy button sits: by default in a row
    /// under the message.
    fn copy(&self, button: Stateful<Div>, _look: &Look<'_>) -> Div {
        div().h(px(18.)).flex().child(button)
    }

    /// What shows under the conversation for whether the agent is working
    /// on a turn: by default nothing, as the header already says so. A row
    /// it draws keeps its height either way, so the list above it does not
    /// change size as turns start and end.
    fn working(&self, _working: bool, _look: &Look<'_>) -> Option<Div> {
        None
    }

    /// How a turn ended, from its description: by default a quiet rule
    /// across the list.
    fn turn(&self, text: &str, look: &Look<'_>) -> Div {
        let rule = || div().flex_1().h(px(1.)).bg(rgb(look.theme.active));
        div()
            .debug_selector(|| "chat-turn".into())
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px((look.text - 1.).max(10.)))
            .text_color(rgb(look.theme.muted))
            .child(rule())
            .child(div().flex_none().child(text.to_owned()))
            .child(rule())
    }
}

pub(super) fn layout_for(mode: ChatLayoutMode) -> &'static dyn ChatLayout {
    match mode {
        ChatLayoutMode::Timeline => &timeline::Timeline,
        ChatLayoutMode::Bubbles => &bubbles::Bubbles,
        ChatLayoutMode::Terminal => &terminal::Terminal,
        ChatLayoutMode::Document => &document::Document,
        ChatLayoutMode::Cards => &cards::Cards,
        ChatLayoutMode::IMessage => &imessage::IMessage,
    }
}

/// How a turn ended, across the row: it breaks a timeline's line, and
/// closes a turn between bubbles.
pub(super) fn summary_row(row: Row<'_>, above: f32) -> AnyElement {
    super::column(
        div()
            .id(("chat-entry", row.entry.id.0))
            .px(px(12.))
            .pt(px(above))
            .pb(px(4.))
            .min_w_0()
            .child(row.content),
    )
    .into_any_element()
}

/// The user's words in a bubble on the right, and the agent's as plain
/// text across the column, as chat apps draw them. `radius` and `padding`
/// shape the bubble, and `gap` is the space where the speaker changes.
pub(super) fn bubble_row(row: Row<'_>, look: &Look<'_>, bubble: Bubble) -> AnyElement {
    let above = if row.speaker() {
        bubble.gap
    } else {
        bubble.gap / 3.
    };
    let id = row.entry.id.0;
    let body = match row.side() {
        Some(Side::User) => {
            let fill = crate::config::mix(look.theme.background, look.theme.foreground, 9);
            div().flex().justify_end().child(
                div()
                    .debug_selector(move || format!("chat-bubble-{id}"))
                    .max_w(relative(0.8))
                    .min_w_0()
                    .px(px(bubble.padding * 1.4))
                    .py(px(bubble.padding))
                    .rounded(px(bubble.radius))
                    .bg(rgb(fill))
                    .text_color(rgb(look.theme.text_on(fill)))
                    .child(row.content),
            )
        }
        _ => div().min_w_0().child(row.content),
    };
    super::column(
        div()
            .id(("chat-entry", id))
            .px(px(16.))
            .pt(px(above))
            .pb(px(2.))
            .min_w_0()
            .child(body),
    )
    .into_any_element()
}

/// The shape of a [`bubble_row`].
#[derive(Clone, Copy)]
pub(super) struct Bubble {
    pub(super) radius: f32,
    pub(super) padding: f32,
    pub(super) gap: f32,
}

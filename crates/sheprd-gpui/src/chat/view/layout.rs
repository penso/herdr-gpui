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
mod ia;
mod imessage;
mod opencode;
mod terminal;
mod timeline;

use super::{copy::CopyPlacement, entries::ToolState};
use crate::{
    SheprdWindow,
    config::{ChatLayoutMode, FontConfig, Theme},
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{Author, Entry, EntryKind};

/// Who an entry speaks for; a turn's summary speaks for neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    User,
    Agent,
}

/// How a message's words are set: who says them, and at what size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Voice {
    pub(super) side: Side,
    pub(super) size: f32,
}

pub(super) fn side(kind: &EntryKind) -> Option<Side> {
    match kind {
        EntryKind::User { .. }
        | EntryKind::Image {
            author: Author::User,
            ..
        } => Some(Side::User),
        EntryKind::Notice { answers, .. } if !answers.is_empty() => Some(Side::User),
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
    /// The entry belongs to the turn since the user's latest prompt.
    pub(super) current: bool,
    /// The user opened the entry.
    pub(super) expanded: bool,
    /// Where the entry stands, when it is a tool call.
    pub(super) tool: Option<ToolState>,
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
    window: &'a SheprdWindow,
}

impl Look<'_> {
    pub(super) fn column(&self, content: impl IntoElement) -> Div {
        self.window.chat_column(content)
    }

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

impl SheprdWindow {
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
    /// Context use, once the agent reports them.
    pub(super) usage: Option<Div>,
    pub(super) send: Stateful<Div>,
    /// The session's directory, once the agent reports it.
    pub(super) place: Option<String>,
    /// How to open the command palette, while it has a shortcut.
    pub(super) commands: Option<Div>,
}

/// How much of a tool call shows until it is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolFold {
    /// Its title, each file's changes, and its first locations and output.
    Preview,
    /// Its title and each file's changes.
    Title,
    /// One line: its title with the changes totalled beside it.
    Line,
}

pub(super) trait ChatLayout {
    /// The entry's row in the list, centered in the reading column. A
    /// turn's summary takes [`summary_row`] in every layout.
    fn row(&self, row: Row<'_>, look: &Look<'_>) -> AnyElement;

    /// The composer's card, from its parts.
    fn composer(&self, parts: ComposerParts, look: &Look<'_>) -> Div;

    /// Where an agent message's copy button sits: by default in a row
    /// under the message, wherever `placement` allows.
    fn copy(&self, button: Stateful<Div>, _placement: CopyPlacement, _look: &Look<'_>) -> Div {
        div().h(px(18.)).flex().child(button)
    }

    /// How much of a tool call in `state` shows until it is opened: by
    /// default its preview.
    fn tool_fold(&self, _state: ToolState) -> ToolFold {
        ToolFold::Preview
    }

    /// Whether messages keep the row's terminal font rather than the
    /// interface's: by default they take the interface's.
    fn mono_prose(&self) -> bool {
        false
    }

    /// Whether the button back to the end of the list says so in words: by
    /// default it is a bare arrow until output lands below.
    fn jump_label(&self) -> bool {
        false
    }

    /// The size the user's prompts are set at: by default the chat's text.
    fn prompt_text(&self, look: &Look<'_>) -> f32 {
        look.text
    }

    /// How far a tool call is inset from the column edge. By default,
    /// nothing is.
    fn tool_indent(&self) -> Option<ToolIndent> {
        None
    }

    /// Whether setting choices stand alone and the model yields width to
    /// the other controls in a single-line footer.
    fn compact_settings(&self) -> bool {
        false
    }

    /// The color an agent message's prose is drawn in: by default the
    /// theme's foreground. `None` keeps the color the row gives it.
    fn prose(&self, look: &Look<'_>) -> Option<u32> {
        Some(look.theme.foreground)
    }

    /// What `side`'s messages sit on, which their accents must read
    /// against: by default the theme's background. `None` is a fill with a
    /// color of its own, where no accent reads and text keeps the row's ink.
    fn ground(&self, _side: Side, look: &Look<'_>) -> Option<u32> {
        Some(look.theme.background)
    }

    /// The fill behind what is drawn for the theme's background inside a
    /// message (a formula's texture, a copy button, a notice) where the
    /// layout fills messages with another color: by default none.
    fn backdrop(&self, _look: &Look<'_>) -> Option<u32> {
        None
    }

    /// What shows under the conversation while the agent works on a turn:
    /// by default a pulsing dot, what it is doing, and how long the turn
    /// has run. A row it draws keeps its height either way, so the list
    /// above it does not change size as turns start and end.
    fn working(&self, working: Option<&super::working::Working>, look: &Look<'_>) -> Option<Div> {
        Some(super::working::row(working, look))
    }

    /// How a turn ended, from its description: by default a quiet rule
    /// across the list.
    fn turn(&self, text: AnyElement, look: &Look<'_>) -> Div {
        let rule = || div().flex_1().h(px(1.)).bg(rgb(look.theme.active));
        div()
            .debug_selector(|| "chat-turn".into())
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px((look.text - 1.).max(10.)))
            .text_color(rgb(look.theme.muted))
            .child(rule())
            .child(div().flex_none().child(text))
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
        ChatLayoutMode::IA => &ia::IA,
        ChatLayoutMode::OpenCode => &opencode::OpenCode,
    }
}

/// How a turn ended, across the row: it breaks a timeline's line, and
/// closes a turn between bubbles.
pub(super) fn summary_row(row: Row<'_>, look: &Look<'_>, above: f32) -> AnyElement {
    look.column(
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

/// The fill of the user's bubble in a [`bubble_row`].
pub(super) fn bubble_fill(look: &Look<'_>) -> u32 {
    crate::config::mix(look.theme.background, look.theme.foreground, 9)
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
            let fill = bubble_fill(look);
            // Measure rich blocks against the available cross-axis width.
            // A horizontal auto flex basis collapses their percentage widths
            // to the Copy button's width before the text is laid out.
            div().flex().flex_col().items_end().child(
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
    look.column(
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

/// The shape of a tool call's inset in a layout.
#[derive(Clone, Copy)]
pub(super) struct ToolIndent {
    /// The padding on the call's row: its header sits this far in.
    pub(super) header: f32,
    /// The padding the call's body (previews, opened details) adds past
    /// `header`.
    pub(super) body: f32,
}

/// The shape of a [`bubble_row`].
#[derive(Clone, Copy)]
pub(super) struct Bubble {
    pub(super) radius: f32,
    pub(super) padding: f32,
    pub(super) gap: f32,
}

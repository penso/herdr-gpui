//! A toast's leading badge. The wire format has no icon field, so a sender
//! picks one by starting the title with an emoji; otherwise the badge shows
//! the sending agent's mark, then an icon for the notification kind.
use crate::notifications::Notice;
use herdr_client::protocol::SemanticNotificationKind;
use unicode_properties::{EmojiStatus, UnicodeEmoji};
use unicode_segmentation::UnicodeSegmentation;

/// Bounds one emoji grapheme, generously enough for ZWJ family sequences.
const EMOJI_BYTES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark<'a> {
    /// A leading emoji the sender put in the title.
    Emoji(&'a str),
    /// An embedded SVG asset path.
    Icon(&'static str),
}

/// The badge's mark and the title left to show beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Badge<'a> {
    pub mark: Mark<'a>,
    pub title: &'a str,
}

pub(super) fn badge(notice: &Notice) -> Badge<'_> {
    if let Some((emoji, title)) = leading_emoji(&notice.title) {
        return Badge {
            mark: Mark::Emoji(emoji),
            title,
        };
    }
    let icon = match (notice.agent, notice.kind) {
        (Some(agent), _) => agent.path(),
        (None, SemanticNotificationKind::NeedsAttention) => "icons/bell.svg",
        (None, SemanticNotificationKind::Finished) => "icons/check.svg",
        (None, SemanticNotificationKind::UpdateInstalled) => "icons/refresh.svg",
        (None, SemanticNotificationKind::Custom) => "icons/note.svg",
    };
    Badge {
        mark: Mark::Icon(icon),
        title: &notice.title,
    }
}

/// Splits `"🚀 Deployed"` into the emoji and the rest. The emoji must be the
/// first grapheme, followed by whitespace and a non-empty title, so a title
/// that merely is or starts with a symbol keeps it.
fn leading_emoji(title: &str) -> Option<(&str, &str)> {
    let first = title.graphemes(true).next()?;
    if first.len() > EMOJI_BYTES || !emoji(first) {
        return None;
    }
    let rest = &title[first.len()..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    (!rest.is_empty()).then_some((first, rest))
}

/// Whether a grapheme presents as an emoji: its base defaults to emoji
/// presentation (🚀, flags), or it is an emoji that a U+FE0F selector asks to
/// present as one (©️, ⚠️, 1️⃣). Symbols like ⌘ and bare digits are text.
fn emoji(grapheme: &str) -> bool {
    let Some(base) = grapheme.chars().next() else {
        return false;
    };
    match base.emoji_status() {
        EmojiStatus::EmojiPresentation
        | EmojiStatus::EmojiPresentationAndModifierBase
        | EmojiStatus::EmojiPresentationAndEmojiComponent => true,
        _ => base.is_emoji_char() && grapheme.contains('\u{FE0F}'),
    }
}

/// The round badge: a neutral disc showing who sent the notice, with a dot
/// in the kind's accent on its corner showing what it is.
pub(super) fn circle(mark: Mark<'_>, theme: &crate::config::Theme, accent: u32) -> gpui::Div {
    use gpui::{ParentElement, Styled, div, px, rgb, svg};
    let disc = div()
        .size_full()
        .rounded_full()
        .bg(rgb(theme.active))
        .flex()
        .items_center()
        .justify_center();
    let disc = match mark {
        Mark::Emoji(emoji) => disc.text_size(px(15.)).child(emoji.to_owned()),
        Mark::Icon(path) => disc.child(
            svg()
                .path(path)
                .size(px(14.))
                .text_color(rgb(theme.foreground)),
        ),
    };
    div()
        .relative()
        .flex_none()
        .size(px(28.))
        .child(disc)
        .child(
            div()
                .absolute()
                .right(px(-2.))
                .bottom(px(-2.))
                .size(px(12.))
                .rounded_full()
                // The ring cuts the dot out of the disc against the card.
                .border_2()
                .border_color(rgb(theme.surface))
                .bg(rgb(accent)),
        )
}

#[cfg(test)]
mod tests;

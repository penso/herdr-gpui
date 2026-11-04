//! File links in a conversation from another host. Their paths name files
//! there, so they are shown as inert text with the path written out after
//! the label; the drawn text and the selectable text both come from here,
//! so a copy matches what is on screen byte for byte.

use super::{Inline, Style};
use crate::terminal::file_target;
use std::borrow::Cow;

/// `content` with each file link made inert and its target appended once.
pub(in crate::chat) fn remote_content(content: &[Inline]) -> Cow<'_, [Inline]> {
    if !content.iter().any(|part| {
        matches!(part, Inline::Text(_, style)
            if style.link.as_deref().is_some_and(|url| file_target(url).is_some()))
    }) {
        return Cow::Borrowed(content);
    }
    let mut shown = Vec::new();
    for (index, part) in content.iter().enumerate() {
        let Inline::Text(text, style) = part else {
            shown.push(part.clone());
            continue;
        };
        let Some(target) = style
            .link
            .as_deref()
            .filter(|url| file_target(url).is_some())
        else {
            shown.push(part.clone());
            continue;
        };
        let mut inert = style.clone();
        inert.link = None;
        shown.push(Inline::Text(text.clone(), inert));
        // Emphasis/code can split one label into several styled text spans.
        // Keep their formatting, then show its destination once at the end.
        if !matches!(content.get(index + 1), Some(Inline::Text(_, next))
            if next.link.as_deref() == Some(target))
        {
            shown.push(Inline::Text(format!(" ({target})"), Style::default()));
        }
    }
    Cow::Owned(shown)
}

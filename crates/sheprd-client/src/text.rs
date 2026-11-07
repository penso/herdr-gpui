//! Checks for untrusted text shown in the app.

use crate::protocol::{ClientShellSnapshot, ClientShellTab};

/// Strip only a leading agent paste envelope, never ordinary markup inside a
/// title. A wrapper-only/truncated transport marker is not a useful caption.
pub fn caption(text: &str) -> Option<&str> {
    let original = text;
    let text = text.trim();
    if text.is_empty() || text == "</pasted_content>" {
        return None;
    }
    let Some(rest) = text.strip_prefix("<pasted_content") else {
        return Some(original);
    };
    if rest
        .chars()
        .next()
        .is_some_and(|c| c != '>' && !c.is_ascii_whitespace())
    {
        return Some(original);
    }
    let end = text
        .as_bytes()
        .iter()
        .take(512)
        .position(|byte| *byte == b'>')?;
    let body = text[end + 1..].trim();
    let body = body
        .strip_suffix("</pasted_content>")
        .unwrap_or(body)
        .trim();
    (!body.is_empty()).then_some(body)
}

/// A tab's display label, with a useful fallback for an agent transport marker.
/// This does not mutate the authoritative snapshot or a user's labels.
pub fn tab_label<'a>(snapshot: &'a ClientShellSnapshot, tab: &'a ClientShellTab) -> &'a str {
    caption(&tab.label)
        .or_else(|| {
            snapshot
                .panes
                .iter()
                .filter(|pane| pane.tab_id == tab.tab_id)
                .find_map(|pane| pane.label.as_deref().and_then(caption))
        })
        .or_else(|| {
            snapshot
                .agents
                .iter()
                .filter(|agent| agent.tab_id == tab.tab_id)
                .find_map(|agent| agent.title.as_deref().and_then(caption))
        })
        .or_else(|| {
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == tab.workspace_id)
                .and_then(|workspace| caption(&workspace.label))
        })
        .unwrap_or("Terminal")
}

/// A control or direction-override character, which untrusted text loses.
pub fn unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{2028}' | '\u{2029}')
}

/// `text` cut to `limit` characters, without the characters `unsafe_char`
/// refuses.
pub fn safe_text(text: &str, limit: usize) -> String {
    // Bound scanning as well as output, even for a payload made entirely of controls.
    text.chars()
        .take(limit)
        .filter(|c| !unsafe_char(*c))
        .collect()
}

#[cfg(test)]
mod tests;

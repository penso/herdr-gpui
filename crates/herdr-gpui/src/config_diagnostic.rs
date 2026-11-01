//! A config diagnostic prepared for display: each endpoint's daemon
//! `config.toml` diagnostic, and this app's own GUI config warnings.
//!
//! Herdr shows `snapshot.config_diagnostic` for as long as the snapshot carries
//! it and clears it with the first snapshot that does not. This mirrors that,
//! adding a dismissal that holds for the exact text the user dismissed, and
//! whether the user opened the card to read lines too long for one row.
use std::sync::Arc;

/// Bounds the scan as well as the output: daemon text is untrusted.
const MAX_CHARS: usize = 1024;
/// Herdr clips the diagnostic to the rows it has; a banner keeps a few.
const MAX_LINES: usize = 4;
const MAX_LINE_CHARS: usize = 240;

#[derive(Debug, Default)]
pub(crate) struct ConfigDiagnostic {
    /// The raw text last seen, so unchanged text is not reprocessed.
    source: Option<String>,
    /// Display lines prepared from `source`, sanitized and bounded.
    lines: Option<Arc<[String]>>,
    dismissed: bool,
    /// The card shows its lines in full rather than one row each.
    expanded: bool,
    /// The locale the app's own text was written in, so a language switch
    /// can write it again. `None` for the daemon's text, which has none.
    locale: Option<herdr_i18n::Locale>,
}

impl ConfigDiagnostic {
    /// Follows the current diagnostic text, `None` once it is gone. The
    /// text is untrusted, as the daemon's is, so it loses control and
    /// direction characters.
    pub(crate) fn sync(&mut self, text: Option<&str>) {
        self.locale = None;
        self.follow(text, |line| {
            crate::notifications::safe_text(line, MAX_LINE_CHARS)
        });
    }

    /// Follows the app's own text, built from messages whose placeables
    /// already lost what untrusted text does. It keeps its isolation and
    /// direction marks, which a right-to-left language needs.
    pub(crate) fn sync_own(&mut self, text: Option<&str>) {
        if self.source.as_deref() == text {
            return;
        }
        self.locale = text.and_then(|_| crate::i18n::shown_locale());
        self.follow(text, own_line);
    }

    /// Writes the app's own text again with `text` once the windows show
    /// another locale than it was written in. The same warning in another
    /// language keeps its dismissal and opened card. Returns whether the
    /// lines changed; nothing is written while there is no own text.
    pub(crate) fn relocale(&mut self, text: impl FnOnce() -> Option<String>) -> bool {
        if self.source.is_none() || self.locale.is_none() {
            return false;
        }
        let locale = crate::i18n::shown_locale();
        if self.locale == locale {
            return false;
        }
        self.locale = locale;
        let Some(text) = text() else {
            self.follow(None, own_line);
            return true;
        };
        let lines = display_lines(&text, own_line);
        let changed = lines != self.lines;
        self.source = Some(text);
        self.lines = lines;
        changed
    }

    /// Whether there is text to show, dismissed or not.
    pub(crate) fn has_text(&self) -> bool {
        self.source.is_some()
    }

    fn follow(&mut self, text: Option<&str>, clean: impl Fn(&str) -> String) {
        if self.source.as_deref() == text {
            return;
        }
        // Gone or changed: a dismissal or an opened card covered only the
        // text it was made on.
        self.dismissed = false;
        self.expanded = false;
        self.source = text.map(str::to_owned);
        self.lines = text.and_then(|text| display_lines(text, clean));
    }

    /// The lines to show, unless dismissed.
    pub(crate) fn visible(&self) -> Option<&Arc<[String]>> {
        self.lines.as_ref().filter(|_| !self.dismissed)
    }

    /// Dismisses `lines` only if they are still what is shown, so a click on a
    /// banner that changed underneath it cannot hide the new text.
    pub(crate) fn dismiss(&mut self, lines: &Arc<[String]>) -> bool {
        let current = self.lines.as_ref().is_some_and(|own| own == lines);
        self.dismissed |= current;
        current
    }

    /// Whether the card shows its lines in full.
    pub(crate) fn expanded(&self) -> bool {
        self.expanded
    }

    /// Opens or closes the card on `lines`, fenced like [`Self::dismiss`]:
    /// a click on a card whose text changed underneath it does nothing.
    pub(crate) fn toggle(&mut self, lines: &Arc<[String]>) -> bool {
        let current = self.visible().is_some_and(|own| own == lines);
        self.expanded ^= current;
        current
    }
}

fn own_line(line: &str) -> String {
    crate::truncate::cut_chars(line, MAX_LINE_CHARS)
        .unwrap_or(line)
        .to_owned()
}

fn display_lines(text: &str, clean: impl Fn(&str) -> String) -> Option<Arc<[String]>> {
    let bounded: String = text.chars().take(MAX_CHARS).collect();
    let lines: Arc<[String]> = bounded
        .lines()
        .map(clean)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned())
        .take(MAX_LINES)
        .collect();
    (!lines.is_empty()).then_some(lines)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    mod expansion;
    mod own_text;
    mod relocale;

    fn shown(state: &ConfigDiagnostic) -> Option<Vec<String>> {
        state.visible().map(|lines| lines.to_vec())
    }

    #[test]
    fn follows_the_snapshot_and_clears_when_it_disappears() {
        let mut state = ConfigDiagnostic::default();
        state.sync(None);
        assert_eq!(shown(&state), None);
        state.sync(Some("config.toml invalid; using defaults"));
        assert_eq!(
            shown(&state),
            Some(vec!["config.toml invalid; using defaults".into()])
        );
        state.sync(None);
        assert_eq!(shown(&state), None);
        state.sync(Some("again"));
        state.sync(None);
        assert_eq!(shown(&state), None);
    }

    #[test]
    fn dismissal_holds_for_the_same_text_only() {
        let mut state = ConfigDiagnostic::default();
        state.sync(Some("first"));
        let lines = state.visible().unwrap().clone();
        assert!(state.dismiss(&lines));
        assert_eq!(shown(&state), None);
        // Later snapshots repeating the text stay dismissed.
        state.sync(Some("first"));
        assert_eq!(shown(&state), None);
        // New text reappears, and a stale dismissal cannot hide it.
        state.sync(Some("second"));
        assert_eq!(shown(&state), Some(vec!["second".into()]));
        assert!(!state.dismiss(&lines));
        assert_eq!(shown(&state), Some(vec!["second".into()]));
        // Returning to the dismissed text is a change too: it shows again.
        state.sync(Some("first"));
        assert_eq!(shown(&state), Some(vec!["first".into()]));
    }

    #[test]
    fn reappears_after_clearing_even_with_the_dismissed_text() {
        let mut state = ConfigDiagnostic::default();
        state.sync(Some("broken"));
        let lines = state.visible().unwrap().clone();
        state.dismiss(&lines);
        state.sync(None);
        state.sync(Some("broken"));
        assert_eq!(shown(&state), Some(vec!["broken".into()]));
    }

    #[test]
    fn text_is_sanitized_and_bounded() {
        let mut state = ConfigDiagnostic::default();
        let text = format!(
            "  client: bad\u{1b}[31m key \u{202e}x\n\n   \nendpoint: {}\n3\n4\n5\n6",
            "y".repeat(10_000)
        );
        state.sync(Some(&text));
        let lines = shown(&state).unwrap();
        assert_eq!(lines[0], "client: bad[31m key x");
        assert!(lines.iter().all(|line| !line.chars().any(char::is_control)));
        assert!(lines.len() <= MAX_LINES);
        assert_eq!(lines[1].chars().count(), MAX_LINE_CHARS);
        // The oversized line consumed the scan budget, so nothing follows it.
        assert_eq!(lines.len(), 2);

        state.sync(Some("\u{7}\n \t \n"));
        assert_eq!(shown(&state), None);
    }
}

//! Completing the SSH target field from the Nearby devices list: which rows
//! match what the user typed, which one Tab and Enter choose, and what
//! choosing one puts in the field.

use super::Suggestion;
use crate::{
    HerdrWindow,
    menu::devices::add_device::{Setup, Step},
};
use gpui::{Context, Window};

/// What the target field holds, split into the user it names and the part
/// that is matched against suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Query<'a> {
    text: &'a str,
    user: Option<&'a str>,
    host: &'a str,
}

impl<'a> Query<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        let text = text.trim_start();
        // `ssh://user@host:port` names its user after the scheme, as a
        // chosen `ssh://` row fills it in.
        let rest = text.strip_prefix("ssh://").unwrap_or(text);
        let (user, host) = match rest.split_once('@') {
            Some((user, host)) if !user.is_empty() && !user.contains('/') => (Some(user), host),
            _ => (None, rest),
        };
        Self { text, user, host }
    }

    /// The same user, listing every row: for a field that holds a row it
    /// already chose, so another can still be picked.
    fn showing_all(self) -> Self {
        Self { host: "", ..self }
    }

    /// Whether the field filters the list: an empty field or a bare `user@`
    /// shows every row.
    pub(super) fn filters(&self) -> bool {
        !self.host.trim().is_empty()
    }

    /// How well `suggestion` matches: names and targets that start with the
    /// typed host rank before those that only contain it. `None` when it does
    /// not match at all.
    fn rank(&self, suggestion: &Suggestion) -> Option<u8> {
        if !self.filters() {
            return Some(0);
        }
        let host = self.host.trim().to_lowercase();
        let target = suggestion.target.to_lowercase();
        let bare = target.strip_prefix("ssh://").unwrap_or(&target);
        let name = suggestion.name.to_lowercase();
        if [bare, name.as_str()].iter().any(|s| s.starts_with(&host)) {
            Some(0)
        } else if [bare, name.as_str()].iter().any(|s| s.contains(&host)) {
            Some(1)
        } else {
            None
        }
    }

    /// The field's text once `suggestion` is chosen: its target, keeping a
    /// user the field already names unless the target carries its own.
    pub(super) fn fill(&self, suggestion: &Suggestion) -> String {
        let target = suggestion.target.as_str();
        let Some(user) = self.user else {
            return target.to_owned();
        };
        if target.contains('@') {
            return target.to_owned();
        }
        match target.strip_prefix("ssh://") {
            Some(rest) => format!("ssh://{user}@{rest}"),
            None => format!("{user}@{target}"),
        }
    }

    /// The rest of `fill` after what is typed, drawn after the caret, when the
    /// typed text is a prefix of it. Matching ignores ASCII case, as host
    /// names do.
    pub(super) fn ghost(&self, fill: &str) -> Option<String> {
        if !self.filters() {
            return None;
        }
        let typed = fill.get(..self.text.len())?;
        let rest = fill.get(self.text.len()..)?;
        (typed.eq_ignore_ascii_case(self.text) && !rest.is_empty()).then(|| rest.to_owned())
    }
}

/// The rows to show for `query`, best matches first and in list order within
/// each rank.
pub(super) fn matching<'s>(
    suggestions: impl IntoIterator<Item = &'s Suggestion>,
    query: &Query,
) -> Vec<&'s Suggestion> {
    let mut ranked: Vec<(u8, &Suggestion)> = suggestions
        .into_iter()
        .filter_map(|suggestion| Some((query.rank(suggestion)?, suggestion)))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .map(|(_, suggestion)| suggestion)
        .collect()
}

/// The list as the target field filters it.
pub(in crate::menu::devices) struct Completion<'s> {
    /// Suggestions not saved as devices yet.
    pub(in crate::menu::devices) unsaved: usize,
    /// Those that match the field, best first.
    pub(super) rows: Vec<&'s Suggestion>,
    /// The highlighted row.
    pub(super) highlight: Option<usize>,
    /// Whether the user moved the highlight there with Up or Down, rather
    /// than it following what they typed.
    pub(super) chosen: bool,
    /// What choosing each row puts in the field, in row order.
    pub(super) fills: Vec<String>,
}

impl Completion<'_> {
    /// The rest of the highlighted row's fill, drawn after the caret.
    pub(in crate::menu::devices) fn ghost(&self, text: &str) -> Option<String> {
        let fill = self.fills.get(self.highlight?)?;
        Query::new(text).ghost(fill)
    }
}

impl HerdrWindow {
    /// The rows for what the target field holds: those that match it, or
    /// every row once it holds one of them. The highlight is the row Up
    /// and Down moved to for this text; otherwise the row already in the
    /// field, or else the first whose target extends what is typed. A row
    /// that only matches by name is never highlighted unasked: Bonjour names
    /// are whatever the network announces, so one must not stand in for a
    /// host the user typed.
    pub(in crate::menu::devices) fn device_completion<'s>(
        &self,
        setup: &'s Setup,
        text: &str,
    ) -> Completion<'s> {
        let unsaved: Vec<&Suggestion> = setup
            .discovery
            .iter()
            .flat_map(|discovery| &discovery.suggestions)
            .filter(|suggestion| {
                !self.endpoints.iter().any(|endpoint| {
                    endpoint
                        .saved_ssh()
                        .is_some_and(|(target, _)| suggestion.saved_as(&target.target))
                })
            })
            .collect();
        let mut query = Query::new(text);
        if unsaved
            .iter()
            .any(|suggestion| query.fill(suggestion) == text)
        {
            query = query.showing_all();
        }
        let rows = matching(unsaved.iter().copied(), &query);
        let fills: Vec<String> = rows.iter().map(|row| query.fill(row)).collect();
        let typed = Query::new(text);
        // An arrow choice names its machine by its discovery keys, since
        // discovery keeps adding and sorting rows while the form is open, and
        // a later source can change the machine's target as it merges in.
        let picked = setup
            .highlight
            .as_ref()
            .filter(|(typed, _)| typed == text)
            .and_then(|(_, keys)| {
                rows.iter()
                    .position(|row| row.keys.iter().any(|key| keys.contains(key)))
            });
        let (highlight, chosen) = match picked {
            Some(index) => (Some(index), true),
            None => (
                fills
                    .iter()
                    .position(|fill| fill == text)
                    .or_else(|| fills.iter().position(|fill| typed.ghost(fill).is_some())),
                false,
            ),
        };
        let highlight = highlight.filter(|index| *index < rows.len());
        Completion {
            unsaved: unsaved.len(),
            rows,
            highlight,
            chosen,
            fills,
        }
    }

    /// Up and Down move the highlight through the rows. Tab puts the
    /// highlighted row in the field: one the user chose, or the completion
    /// drawn after the caret. Enter does so only for a row the user chose, so
    /// it never sends anything but what the field shows. Returns whether the
    /// key was used, so Tab and Enter otherwise keep their jobs.
    pub(in crate::menu::devices) fn complete_device_target(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(setup) = &self.menu.device_setup else {
            return false;
        };
        if !matches!(setup.step, Step::Form) {
            return false;
        }
        let text = setup.fields[0].read(cx).text().to_owned();
        let completion = self.device_completion(setup, &text);
        match key {
            "up" | "down" => {
                let Some(last) = completion.rows.len().checked_sub(1) else {
                    return false;
                };
                let index = match (completion.highlight, key) {
                    (None, "up") => last,
                    (None, _) => 0,
                    (Some(index), "up") => index.saturating_sub(1),
                    (Some(index), _) => (index + 1).min(last),
                };
                let Some(keys) = completion.rows.get(index).map(|row| row.keys.clone()) else {
                    return false;
                };
                if let Some(setup) = &mut self.menu.device_setup {
                    setup.suggestion_scroll.scroll_to_item(index);
                    setup.highlight = Some((text, keys));
                }
                cx.notify();
                true
            }
            "tab" | "enter" => {
                let Some(index) = completion.highlight else {
                    return false;
                };
                let (Some(fill), Some(row)) =
                    (completion.fills.get(index), completion.rows.get(index))
                else {
                    return false;
                };
                let completes =
                    completion.chosen || (key == "tab" && Query::new(&text).ghost(fill).is_some());
                if *fill == text || !completes {
                    return false;
                }
                let (fill, name) = (fill.clone(), row.name.clone());
                self.use_device_suggestion(&fill, &name, window, cx);
                true
            }
            _ => false,
        }
    }
}

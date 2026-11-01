//! The bar's count bubbles: what needs the user, as the Overview counted it
//! on the last tick, and from the orchestrator views the window holds, open
//! pull requests and open issues, the open critical and high Dependabot
//! alerts, and the automation scripts waiting for approval or after a
//! failed run. An item two views list, such as one repository open in two
//! workspaces, counts once. Each count keeps when its newest item last
//! changed, which the bar compares with when the section was last open.

use super::ActivityPanel;
use crate::{HerdrWindow, orchestrator::Item};
use chrono::{DateTime, Utc};
use gpui::App;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Counts {
    needs: Count,
    pull_requests: Count,
    issues: Count,
    security: Count,
    automations: Count,
}

/// How many items a bubble counts, and when the newest of them changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Count {
    items: usize,
    latest: Option<DateTime<Utc>>,
}

impl Counts {
    /// One view's items added to the counts, skipping what `seen` holds.
    /// A copy already counted still offers its time, since views sync on
    /// their own and a later one may hold the newer copy.
    pub(super) fn add(&mut self, seen: &mut HashSet<String>, items: &[Item]) {
        for item in items {
            if !open(item) {
                continue;
            }
            let count = if item.pull_request.is_some() {
                &mut self.pull_requests
            } else {
                &mut self.issues
            };
            if seen.insert(item.key.canonical()) {
                count.items += 1;
            }
            count.latest = count.latest.max(item.updated_at.or(item.created_at));
        }
    }

    fn count(self, panel: ActivityPanel) -> Option<Count> {
        match panel {
            ActivityPanel::Overview => Some(self.needs),
            ActivityPanel::Workspaces | ActivityPanel::Ports | ActivityPanel::Usage => None,
            ActivityPanel::PullRequests => Some(self.pull_requests),
            ActivityPanel::Issues => Some(self.issues),
            ActivityPanel::Security => Some(self.security),
            ActivityPanel::Automations => Some(self.automations),
        }
    }

    /// The bar's count for `panel`, if it shows one.
    pub(super) fn of(self, panel: ActivityPanel) -> Option<usize> {
        self.count(panel).map(|count| count.items)
    }

    /// When the newest item `panel` counts changed, when known.
    pub(super) fn latest(self, panel: ActivityPanel) -> Option<DateTime<Utc>> {
        self.count(panel)?.latest
    }
}

/// Whether the provider still counts the item as open. Beads' own word for
/// finished work counts as closed too.
fn open(item: &Item) -> bool {
    !matches!(item.state.as_str(), "closed" | "merged" | "done")
}

impl HerdrWindow {
    pub(super) fn activity_counts(&self, cx: &App) -> Counts {
        let mut counts = Counts {
            needs: Count {
                items: self.overview_needing(),
                latest: self.overview_needing_since().map(DateTime::from),
            },
            security: Count {
                items: self.security.urgent(),
                latest: self.security.urgent_since(),
            },
            automations: Count {
                items: self.automations.needing(),
                latest: self.automations.needing_since(),
            },
            ..Counts::default()
        };
        let mut seen = HashSet::new();
        for (_, view) in self.orchestrator_views() {
            for (_, items, _) in view.read(cx).listed() {
                counts.add(&mut seen, items);
            }
        }
        counts
    }
}

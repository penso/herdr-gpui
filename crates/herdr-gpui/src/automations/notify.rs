//! Telling the user, once, that a host wants them: a script newly waiting
//! for approval, or a run that newly failed, halted, or stopped. The first
//! read of a host only learns what is already there, so opening the app
//! does not replay every old failure.

use super::model;
use crate::{HerdrWindow, i18n::fl};
use gpui::Context;
use herdr_automation::report::{ScriptState, Status};
use std::collections::HashSet;

/// What a host's notifications already covered.
#[derive(Debug, Default)]
pub(crate) struct Seen {
    initialized: bool,
    /// Scripts waiting for approval, by file and content.
    approvals: HashSet<(String, String)>,
    /// Runs that ended badly, by ID.
    failures: HashSet<i64>,
}

/// One notification to post.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notice {
    pub(crate) title: String,
    pub(crate) body: String,
}

impl Seen {
    /// Learns `status`; what is new since the last read.
    pub(crate) fn observe(&mut self, host: &str, status: &Status) -> Vec<Notice> {
        let approvals: HashSet<(String, String)> = status
            .scripts
            .iter()
            .filter(|script| script.state == ScriptState::AwaitingApproval)
            .map(|script| (script.path.clone(), script.hash.clone()))
            .collect();
        let failed = status
            .runs
            .iter()
            .chain(
                status
                    .scripts
                    .iter()
                    .filter_map(|script| script.last_run.as_ref()),
            )
            .filter(|run| model::failed(run.state));
        let mut notices = Vec::new();
        let mut failures = HashSet::new();
        for run in failed {
            if !failures.insert(run.id) || !self.initialized || self.failures.contains(&run.id) {
                continue;
            }
            notices.push(Notice {
                title: fl!(
                    "automations-notify-failed",
                    name = run.name.as_str(),
                    host = host
                ),
                body: run
                    .detail
                    .clone()
                    .unwrap_or_else(|| fl!("automations-notify-unfinished")),
            });
        }
        if self.initialized {
            for script in &status.scripts {
                let key = (script.path.clone(), script.hash.clone());
                if approvals.contains(&key) && !self.approvals.contains(&key) {
                    notices.push(Notice {
                        title: fl!(
                            "automations-notify-approval",
                            name = script.name.as_str(),
                            host = host
                        ),
                        body: fl!("automations-notify-approval-body"),
                    });
                }
            }
        }
        // Failures stay remembered while the host lists them, so one that
        // falls out of the recent runs and back is not news.
        self.failures.extend(failures);
        self.approvals = approvals;
        self.initialized = true;
        notices
    }
}

impl HerdrWindow {
    /// Posts each notice as the app's own toast, unless notifications are
    /// off.
    pub(super) fn post_automation_notices(&mut self, notices: Vec<Notice>, cx: &mut Context<Self>) {
        if self.config.notifications.delivery() == crate::config::NotificationDelivery::Off {
            return;
        }
        for notice in notices {
            self.local_transfer_notice(&notice.title, notice.body, cx);
        }
    }
}
